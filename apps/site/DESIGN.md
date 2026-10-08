# Quality documentation design

Quality shares the sculpted composition of the santi020k website and theme:
clear typography, a compact floating navigation dock, one aligned grid, open numbered rows,
quiet reading surfaces, and short purposeful motion. It retains its own blue
identity and the deterministic developer-tool character of the product.

## Shared contracts

- `src/styles/global.css` owns Quality's public Lumen semantic token overrides,
  shared gutters, reading styles, header, and footer. The existing blue palette is
  explicit so future Lumen defaults cannot silently rebrand the site.
- `src/components/SiteHeader.astro` uses Lumen navigation, theme, button, and
  dialog primitives. Its solid dock keeps the identity, centered desktop links, and
  grouped utilities together while scrolling. The mobile dialog owns focus and
  dismissal through Lumen.
- `src/lib/docs-navigation.ts` is the source of guide order for the sidebar,
  mobile menus, previous/next links, and language-model documentation endpoints.
- `src/pages/index.astro` owns homepage composition and its scoped styles.
  The terminal is explicitly labeled as an example. Lumen Code and CodeTabs
  supply copy controls and keyboard interaction.
- `src/pages/[slug].astro` applies one reading template to all eleven guides.
  Tables of contents come from rendered headings; every guide exposes its
  Markdown source and neighboring pages. Article content is never hidden behind
  reveal animation.
- `SiteLayout.astro` mounts Lumen's runtime once and Astro's ClientRouter.
  Page content uses a short fade; the selected theme is applied before page
  swaps. Reduced motion removes animation and smooth scrolling.
- `src/scripts/site.ts` owns only consumer lifecycle behavior: preserve theme
  during swaps, dismiss navigation before routing, and make overflowing reading
  examples keyboard reachable. Lumen owns primitive interaction behavior.
- The existing Satteri Markdown processor applies protected external links at
  build time. Internal routes, fragments, and downloads keep native navigation.

## Verification

Run `pnpm --filter @quality/site build` then `pnpm test:site`. The browser suite
checks the homepage and every guide in light/dark themes at desktop/mobile sizes,
with accessibility scans, overflow checks, keyboard navigation, theme changes,
page swaps, history, protected external links, and narrow enlarged-text layouts.

For local Chrome, use `PW_CHROMIUM_CHANNEL=chrome pnpm test:site`. Otherwise
install Chromium with `pnpm --filter @quality/site exec playwright install
chromium`. CI installs browser system dependencies and runs the same suite.
Before/after screenshots are temporary verification evidence outside the repo.

No public documentation route, CLI contract, report format, or machine-readable
Markdown endpoint changes as part of this redesign.
