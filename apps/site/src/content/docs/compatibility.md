---
title: Compatibility and support
description: Understand quality's stable contracts, exit codes, supported platforms, and upgrade policy.
---

## Stable contracts

`quality.yml` has its own integer `version`. A release rejects configuration
versions it cannot interpret instead of silently changing their meaning.

The native JSON produced by `check`, `format`, `fix`, `doctor`, and
`repositories` commands includes `schema_version: 1`. Consumers should require
a schema version they understand and ignore unknown object fields so additive
fields remain compatible.

Published schemas describe the current contract:

- [`check`, `format`, and `fix` reports](/quality-report.schema.json)
- [`doctor` reports](/quality-doctor.schema.json)
- [`repositories` reports](/quality-repositories.schema.json)
- [`quality.yml`](/quality.schema.json)

Within a stable CLI major release, existing command names, flags, built-in
adapter IDs, GitHub Action input and output names, and fields required by the
published report schemas remain compatible. New optional fields, adapters, and
flags may be added. Removing or changing the meaning or type of an existing
contract requires a new CLI major release or report schema version.

Preset output is intentionally upgradeable. A minor release may update pinned
analyzer versions or generated policy, but `quality preset diff` and
`quality preset update --dry-run` expose the change before files are replaced.

## Exit codes

| Code | Meaning |
| ---: | --- |
| `0` | The command completed and its selected policy was satisfied. |
| `1` | The command completed but found diagnostics, missing required tools, failed configured checks, preset drift, or selected repository-audit conditions. |
| `2` | The command could not complete because its invocation, configuration, project discovery, or I/O was invalid. |

Do not parse human-readable output to distinguish results. Use JSON or SARIF
and inspect normalized statuses, `failure_kind`, diagnostics, and summaries.

## Supported platforms

Prebuilt releases support:

- macOS on Apple Silicon and Intel
- Linux on ARM64 and x86-64 with glibc
- Windows on x86-64

The CLI and GitHub Action are tested on Linux, macOS, and Windows. Individual
analyzers retain their own platform requirements; for example, Swift checks
normally require macOS and Android checks require a compatible JDK and Android
toolchain.

The latest stable release receives fixes. During the pre-1.0 preview, only the
latest preview release is supported. Analyzer compatibility is exercised by
the repository's real-adapter CI using pinned representative versions, while
repository-local command and argument overrides remain available when an
upstream analyzer requires a different invocation.

## Upgrade and rollback

Pin exact release tags in CI for reproducibility. Review the changelog and
Changesets, update the pinned version, then run `quality doctor` and the full
repository gate. Keep the previous binary or Action tag available so rollback
only requires restoring that pin; configuration and baseline formats are
rejected explicitly when unsupported rather than migrated destructively.
