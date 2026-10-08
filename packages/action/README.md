<p align="center">
  <a href="../../README.md"><img src="../../apps/site/public/logo.svg" alt="quality" width="80"></a>
</p>

<p align="center"><a href="../../README.md">quality</a></p>

<h1 align="center">GitHub Action</h1>

<p align="center">
  <a href="../../README.md">Project overview</a> ·
  <a href="package.json">Package manifest</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="#resources">Resources</a>
</p>

**On this page:** [Use the action](#use-the-action) · [Source map](#source-map) · [Working on the action](#working-on-the-action) · [Workspace commands](#workspace-commands) · [Resources](#resources)

Run Quality in GitHub Actions with native annotations and SARIF output.

## Use the action

The public entry point is the repository-root [action.yml](../../action.yml), not this package path.
It defines the supported inputs, outputs, runtime, and bundled JavaScript entry.
See the [root guide](../../README.md) for a complete workflow example and release pinning.

## Source map

| File | Responsibility |
| --- | --- |
| [src/main.ts](src/main.ts) | GitHub Actions entry point. |
| [src/lib.ts](src/lib.ts) | Installation and execution helpers. |
| [action.yml](../../action.yml) | Public action contract. |
| [dist](dist) | Checked-in bundle used by action consumers. |

## Working on the action

Run `build` to regenerate the bundle after source changes; do not edit `dist` by hand.
The root `check:action-dist` command verifies that tracked output matches the source.
This private workspace is not an npm installation target.

## Workspace commands

Run from the repository root after its documented setup. Use the Node.js and pnpm versions
declared in the root [package.json](../../package.json).

| Task | Command |
| --- | --- |
| Build or compile this workspace | `pnpm --filter @quality/action run build` |
| Run workspace diagnostics | `pnpm --filter @quality/action run check` |
| Run workspace tests | `pnpm --filter @quality/action run test` |

## Resources

[Project overview](../../README.md) · [Contributing](../../CONTRIBUTING.md) · [License](../../LICENSE)
