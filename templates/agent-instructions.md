## Code quality

- Run `quality doctor` after changing quality tooling or `quality.yml`.
- Run `quality check --changed` for fast feedback while editing.
- Run the complete `quality check` before handoff.
- Treat every reported diagnostic as work to resolve.
- Use `quality fix` only when modifications are intended, then inspect its changes.
- Do not bypass configured checks or weaken `quality.yml` merely to obtain a passing result.
