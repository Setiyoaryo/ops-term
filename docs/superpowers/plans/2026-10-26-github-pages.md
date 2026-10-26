# OpsTerm GitHub Pages landing page

Goal: publish a polished, responsive product landing page at https://setiyoaryo.github.io/ops-term/ with an OpenCode-inspired terminal editorial style and original OpsTerm branding.

Design: charcoal grid, warm off-white mono typography, muted peach signal accent, large two-line hero, versioned download CTA, a clearly labelled illustrative workspace with session/palette switches, concise protocol/features/security sections, verified install commands, FAQ and a clean footer. Light-mode control is optional presentation; no fake statistics, customer claims or live SSH behavior.

Architecture: standalone static HTML/CSS/JS in website/, separate from the desktop React frontend. A small Python builder validates GitHub release metadata and inserts safe versioned URLs into dist/pages. Assets include an original SVG mark and a self-hosted official JetBrains Mono webfont with its OFL license. No framework, runtime third-party scripts, analytics or external font requests.

- [x] Prove release metadata/URL validation with failing tests, then implement the builder and landing page.
- [x] Test real browser behavior: navigation, install/source copy, palette/session previews, FAQ, desktop/mobile layouts, console/network errors and no horizontal viewport overflow. Inspect screenshots.
- [x] Review code and publishing permissions. Add a GitHub Pages workflow triggered by website changes, manual dispatch and successful release workflow completion; refresh release metadata at build time.
- [ ] Enable Pages, publish the verified commit, confirm the live HTTPS page and set the repo homepage link. Clean generated output after artifacts/screenshots are used.

Verification before publication: 3 metadata/template tests and 8 desktop/mobile Playwright checks passed; desktop/mobile screenshots inspected; actionlint passed. Independent review found a build-prerequisites link and missing D-Bus development dependency in README; both corrected.
