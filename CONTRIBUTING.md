# Contributing

Install Rust 1.85 or newer, Node.js 22.18 or newer, and pnpm 11.22. Then run:

```sh
pnpm install --frozen-lockfile
pnpm validate
```

Add tests for behavior changes and a Changeset for user-visible changes:

```sh
pnpm changeset
```

Pull requests should keep the CLI deterministic, resolve all diagnostics, and
rebuild the committed GitHub Action bundle whenever `packages/action/src`
changes.
