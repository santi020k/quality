---
title: Language-aware presets
description: Generate, inspect, upgrade, and safely merge analyzer configuration across supported ecosystems.
---

Presets configure the ecosystem tools that `quality` orchestrates. They remain
outside the checking path: applying a preset writes ordinary, reviewable config
files and pinned dependency declarations, while `quality check` continues to
run deterministic repository-local tools.

## Profiles

- `minimal` enables essential analyzers with low-ceremony defaults. Its ESLint
  configuration uses the `basic` preset from
  `@santi020k/eslint-config-basic`.
- `recommended` adds balanced formatting, spelling, unused-code, and complexity
  policy. ESLint retains recommended severities.
- `strict` tightens size and complexity limits, promotes warnings where the
  analyzer supports it, and uses ESLint's `pedantic` strict mode.

Preview and apply a profile:

```bash
quality preset apply recommended --dry-run
quality preset apply recommended --install
```

Detection covers JavaScript/TypeScript/Astro, Python, Rust, Swift, Kotlin/Android, and
GitHub Actions. Limit generation when needed:

```bash
quality preset apply strict --only javascript,rust
```

## Ownership and merging

Every application writes `.quality-preset.json` against the published
`quality-preset.schema.json`. It records the schema and catalog versions,
profile, ecosystems, dependency pins, and a fingerprint for each generated
file.

The ownership rules are deliberately conservative:

- `quality.yml` is parsed and merged. Existing tasks, hooks, custom adapters,
  output selection, and baseline path survive preset updates.
- `.editorconfig` receives a section between `quality-preset:start` and
  `quality-preset:end` markers. Settings outside those markers remain owned by
  the repository.
- Whole-file configs such as `eslint.config.mjs`, `.swiftlint.yml`, and
  `rustfmt.toml` are replaceable only while their fingerprint matches the last
  generated version. User edits block an update unless `--force` is supplied.

Changing profiles removes obsolete generated files only when their recorded
fingerprints prove that they remain untouched.

## Upgrades and compatibility

Inspect the applied state without writing:

```bash
quality preset diff
quality preset update --dry-run
```

`preset diff` exits successfully when the preset is current and with status 1
when it finds file, dependency, or catalog drift. After review, update it:

```bash
quality preset update
quality preset update --install
```

`quality doctor` reports the profile and catalog state. Metadata from a newer,
unsupported schema or catalog is marked incompatible rather than changed
automatically.

## JavaScript frameworks

Recommended and strict profiles inspect workspace package manifests and add
explicit config packs for Angular, Astro, Expo, Hono, Lit, Nest, Next, Nuxt,
Preact, Qwik, React, React Router, Slidev, Solid, Svelte, TanStack Start, Vite,
and Vue. Explicit packs avoid hidden optional dependencies while retaining
framework-specific rules.

## Spelling policy

Presets enable only one spelling adapter to avoid duplicate findings. CSpell is
selected for JavaScript repositories, Codespell for Python repositories, and
Typos for other recommended or strict native-language repositories. Repositories
can override that choice explicitly in `quality.yml`.

## Native setup

Print setup guidance for the ecosystems recorded in the preset:

```bash
quality preset setup
```

The report includes pinned JavaScript dependencies, Rust toolchain components,
SwiftLint/SwiftFormat, detekt/ktlint, Android's Gradle and Java requirements,
Codespell or Typos when selected, and Actionlint. To run supported platform
commands explicitly:

```bash
quality preset setup --install
```

Gradle and SwiftPM integration remains guidance-only when modifying a build
manifest would require project-specific decisions.
