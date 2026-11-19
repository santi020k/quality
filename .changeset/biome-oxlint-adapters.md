---
"@quality/cli": minor
"@quality/action": minor
"@quality/site": minor
---

Add built-in `biome` and `oxlint` adapters. Both are detected from dependencies,
package scripts, or their configuration files, prefer the workspace-local
executable, and support changed-file runs. Biome checks, formats, and fixes;
Oxlint checks and fixes. Choose the JavaScript stack per repository with
`tools.<id>.enabled`, and avoid enabling Prettier and the Biome formatter for the
same files.

Add the `github-annotations` parser for GitHub workflow-command output, available
to custom adapters, tasks, and hook steps.

Prepare the Pawl integration (beta): repositories that use `@santi020k/pawl`
or a `pawl.config.*` file no longer auto-enable the `biome`, `oxlint`,
`eslint`, or `prettier` adapters, because Pawl already runs those engines
against its own baseline. Enabling an adapter in `quality.yml` still runs it.
