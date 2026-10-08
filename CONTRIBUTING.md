# Contributing

Install Rust 1.85 or newer, Node.js 24 (the CI version), and pnpm 11.22. The
inherited Node engine-floor discrepancy is tracked in [Releasing](RELEASING.md).
Then run:

```sh
pnpm install --frozen-lockfile
pnpm validate
```

For documentation UI changes, also run `pnpm test:site` after the production
build. Install its browser with `pnpm --filter @quality/site exec playwright
install chromium`, or use local Chrome with `PW_CHROMIUM_CHANNEL=chrome pnpm
test:site`. See [the site design guide](apps/site/DESIGN.md) for visual and
accessibility contracts.

Add tests for behavior changes and a Changeset for user-visible changes:

```sh
pnpm changeset
```

Pull requests should keep the CLI deterministic, resolve all diagnostics, and
rebuild the committed GitHub Action bundle whenever `packages/action/src`
changes.
