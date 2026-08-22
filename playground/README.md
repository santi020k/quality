# quality playground

This playground is a disposable project for manually trying `quality`. Its mock
analyzers behave like real external tools but require only Git, a POSIX shell,
and the Rust toolchain already used by this repository.

## Start fresh

From the repository root:

```bash
pnpm playground:setup
```

This creates `playground/.sandbox` and initializes it as a standalone Git
repository. The command refuses to overwrite an existing sandbox. To start over,
remove `playground/.sandbox` and run setup again.

## Walk through the main features

Check tool discovery:

```bash
pnpm playground -- doctor
```

Run the analyzers. The initial lint issue is intentional, so this exits with a
failure and prints a normalized diagnostic:

```bash
pnpm playground -- check
```

Try the other output formats and write a SARIF report:

```bash
pnpm playground -- check --format json
pnpm playground -- check --report reports/quality.sarif
```

The initial issue is a warning. It can remain visible without failing the run
when only errors should block development:

```bash
pnpm playground -- check --report-level warning --fail-level error
```

To try an error-level finding, add a line containing `QUALITY_ERROR` to a
`.demo` file. The mock linter intentionally recognizes both marker levels.

Record the known lint issue, then confirm it is suppressed:

```bash
pnpm playground -- baseline create
pnpm playground -- check
```

Edit `playground/.sandbox/src/greeting.demo` and add another line containing
`QUALITY_WARNING`. The next check reports only that new regression.

Because the sandbox has its own Git history, changed-file mode is also safe to
try:

```bash
pnpm playground -- check --changed
```

Remove lint markers and check formatting:

```bash
pnpm playground -- fix
pnpm playground -- format --check
pnpm playground -- format
pnpm playground -- format --check
```

The fixture's `src/message.prose` starts with trailing whitespace, so the first
formatting check is expected to fail and the final one is expected to pass.

## Verify the playground itself

This command creates a separate temporary sandbox and checks discovery,
warning/error thresholds, diagnostics, fixes, formatting, changed-file mode,
baselines, and SARIF output:

```bash
pnpm playground:verify
```

The temporary sandbox is removed automatically.

The verifier is also part of the root `pnpm test` command and the CI pipeline,
so playground drift is caught alongside product regressions.
