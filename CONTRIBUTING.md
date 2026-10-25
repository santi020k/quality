# Contributing

Install Rust 1.85 or newer, Node.js 24 (the CI version), and pnpm 11.22. The
inherited Node engine-floor discrepancy is tracked in [Releasing](RELEASING.md).
Then run:

```sh
pnpm install --frozen-lockfile
pnpm validate
```

For faster feedback while editing, preview and run the repository's affected CI
pilot. Choose the branch you are integrating into as the base; use `HEAD` for
uncommitted edits only:

```sh
pnpm ci:plan --base HEAD
pnpm run ci --base HEAD
pnpm run ci --base origin/main --report /tmp/quality-ci.json
```

This pilot uses Quality's changed checks and Turbo's affected package graph.
It retains the live dependency audit and release-reference checks. Shared or
unclassified inputs and missing Git history fall back to full validation.
`pnpm validate`, pre-push, and release validation remain full gates. See the
[pilot scope and benchmark procedure](docs/affected-ci-pilot.md) before adopting
the policy in another repository.

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
