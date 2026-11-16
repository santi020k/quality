---
"@quality/cli": patch
---

Stop a configured timeout (or a normal exit) from hanging indefinitely when
a command's stdout or stderr pipe is inherited by a descendant process, such
as a background job under `sh -c '... &'`. Output capture is now bounded,
and any capture left unfinished by that bound is marked truncated even when
the retained prefix happens to parse cleanly, so a truncated failure can
never be treated as safe to baseline.

Timeouts terminate the direct command. Descendant processes may continue, while
their open output pipes no longer keep the CLI blocked.
