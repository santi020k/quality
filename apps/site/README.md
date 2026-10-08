<p align="center">
  <a href="../../README.md"><img src="public/logo.svg" alt="quality" width="80"></a>
</p>

<p align="center"><a href="../../README.md">quality</a></p>

<h1 align="center">Documentation</h1>

<p align="center">
  <a href="../../README.md">Project overview</a> ·
  <a href="package.json">Package manifest</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="#resources">Resources</a>
</p>

**On this page:** [Purpose](#purpose) · [Source map](#source-map) · [Verification](#verification) · [Workspace commands](#workspace-commands) · [Resources](#resources)

Documentation for one consistent code-quality workflow.

## Purpose

The Astro site documents Quality's CLI, adapters, configuration, and GitHub integration.
It also exposes Markdown and language-model-friendly documentation endpoints from the same content.

## Source map

| Area | Responsibility |
| --- | --- |
| [src/content.config.ts](src/content.config.ts) | Documentation content collection. |
| [src/pages](src/pages) | HTML, Markdown, sitemap, and language-model endpoints. |
| [src/lib](src/lib) | Navigation and documentation helpers. |
| [scripts](scripts) | Social image generation and link behavior. |

## Verification

Run `check` for Astro diagnostics and `test` for documentation behavior. Browser tests run through
`test:browser`; the root `test:site` command exercises the site against its production build.

## Workspace commands

Run from the repository root after its documented setup. Use the Node.js and pnpm versions
declared in the root [package.json](../../package.json).

| Task | Command |
| --- | --- |
| Start local development | `pnpm --filter @quality/site run dev` |
| Build or compile this workspace | `pnpm --filter @quality/site run build` |
| Run workspace diagnostics | `pnpm --filter @quality/site run check` |
| Run workspace tests | `pnpm --filter @quality/site run test` |

## Resources

[Project overview](../../README.md) · [Contributing](../../CONTRIBUTING.md) · [License](../../LICENSE)
