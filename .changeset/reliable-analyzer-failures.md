---
"@quality/cli": patch
---

Keep existing findings visible when an analyzer fails with output that cannot be
fully parsed, so an existing baseline cannot turn an execution failure into a
passing check.

Keep unsuccessful analyzer runs with unrecognized output failing even when the
failure threshold is limited to errors.

Respect inherited Git hook-manager settings when installing or checking managed
hooks, including global and conditional Git configuration.

Pass changed filenames beginning with a dash as explicit relative paths so
file-scoped analyzers cannot interpret them as command-line options.

Use comma-separated changed inputs for the stable detekt CLI. Fall back to the
complete project when a filename contains a comma instead of sending an
ambiguous input list.

Treat fully parsed analyzer findings as code issues even when their messages
mention resource or environment failures.
