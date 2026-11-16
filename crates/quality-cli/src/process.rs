//! Timed analyzer ownership and Unix terminal-signal forwarding.

use std::io;
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

#[cfg(unix)]
use std::collections::BTreeSet;
#[cfg(unix)]
use std::sync::Mutex;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::Instant;

#[cfg(unix)]
static ACTIVE_GROUPS: Mutex<BTreeSet<u32>> = Mutex::new(BTreeSet::new());

/// Owns the signal thread for the entire CLI invocation. It must be installed
/// before worker threads start and dropped only as `main` returns: signal-hook
/// unregisters callbacks, but cannot restore prior handlers without races.
#[cfg(unix)]
pub struct SignalForwarding {
    handle: signal_hook::iterator::Handle,
    thread: Option<thread::JoinHandle<()>>,
}

#[cfg(unix)]
impl SignalForwarding {
    pub fn install() -> io::Result<Self> {
        use signal_hook::consts::signal::{SIGCONT, SIGHUP, SIGINT, SIGQUIT, SIGTERM, SIGTSTP};

        let mut signals = signal_hook::iterator::Signals::new([
            SIGINT, SIGTERM, SIGHUP, SIGQUIT, SIGTSTP, SIGCONT,
        ])?;
        let handle = signals.handle();
        let thread = thread::Builder::new()
            .name("quality-signals".to_owned())
            .spawn(move || {
                for signal in signals.forever() {
                    // Spawning, registering, and reaping use this same lock.
                    // No recorded PID can be reused before forwarding ends.
                    let groups = ACTIVE_GROUPS
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if !groups.is_empty() {
                        let mut command = Command::new("/bin/kill");
                        command.arg(format!("-{signal}")).arg("--");
                        command.args(groups.iter().map(|pid| format!("-{pid}")));
                        if let Err(error) = run_cleanup_command(command) {
                            eprintln!("quality: could not forward signal to analyzers: {error}");
                        }
                    }
                    // Preserve signal termination (e.g. SIGINT), rather than
                    // reporting a successful check or a generic exit status.
                    if let Err(error) = signal_hook::low_level::emulate_default_handler(signal) {
                        eprintln!("quality: could not restore signal termination: {error}");
                        std::process::exit(128 + signal);
                    }
                }
            })?;
        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }
}

#[cfg(unix)]
impl Drop for SignalForwarding {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct AnalyzerProcess {
    child: Child,
    #[cfg(unix)]
    group: Option<u32>,
}

impl AnalyzerProcess {
    pub fn spawn(mut command: Command, timed: bool) -> io::Result<Self> {
        #[cfg(unix)]
        if timed {
            use std::os::unix::process::CommandExt;

            let mut groups = ACTIVE_GROUPS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let child = command.process_group(0).spawn()?;
            groups.insert(child.id());
            return Ok(Self {
                group: Some(child.id()),
                child,
            });
        }
        #[cfg(not(unix))]
        let _ = timed;
        Ok(Self {
            child: command.spawn()?,
            #[cfg(unix)]
            group: None,
        })
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        #[cfg(unix)]
        debug_assert!(self.group.is_none());
        self.child.wait()
    }

    pub fn wait_timeout(&mut self, timeout: Duration) -> io::Result<Option<ExitStatus>> {
        #[cfg(unix)]
        if let Some(pid) = self.group {
            let started = Instant::now();
            loop {
                {
                    let mut groups = ACTIVE_GROUPS
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some(status) = self.child.try_wait()? {
                        groups.remove(&pid);
                        self.group = None;
                        return Ok(Some(status));
                    }
                }
                let remaining = timeout.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    return Ok(None);
                }
                thread::sleep(remaining.min(Duration::from_millis(10)));
            }
        }
        self.child.wait_timeout(timeout)
    }

    /// Kill descendants before reaping their parent. Children that deliberately
    /// detach from the process group/tree are outside this cleanup boundary.
    pub fn terminate(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        let tree_result = {
            let mut command = Command::new("/bin/kill");
            command.args(["-KILL", "--", &format!("-{}", self.child.id())]);
            run_cleanup_command(command)
        };
        #[cfg(windows)]
        let tree_result = {
            let mut command = Command::new("taskkill.exe");
            command.args(["/PID", &self.child.id().to_string(), "/T", "/F"]);
            run_cleanup_command(command)
        };
        #[cfg(not(any(unix, windows)))]
        let tree_result = Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "process-tree termination is unavailable on this platform",
        ));

        if tree_result.is_err() {
            self.child.kill()?;
        }
        tree_result
    }
}

#[cfg(unix)]
impl Drop for AnalyzerProcess {
    fn drop(&mut self) {
        if let Some(pid) = self.group {
            ACTIVE_GROUPS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&pid);
        }
    }
}

#[cfg(any(unix, windows))]
fn run_cleanup_command(mut command: Command) -> io::Result<()> {
    let mut cleanup = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    match cleanup.wait_timeout(Duration::from_secs(5)) {
        Ok(Some(status)) if status.success() => Ok(()),
        Ok(Some(status)) => Err(io::Error::other(format!(
            "process-tree cleanup exited with {status}"
        ))),
        result => {
            let _ = cleanup.kill();
            let _ = cleanup.wait_timeout(Duration::from_secs(1));
            Err(match result {
                Err(error) => error,
                _ => io::Error::new(io::ErrorKind::TimedOut, "process-tree cleanup timed out"),
            })
        }
    }
}
