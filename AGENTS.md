# Agent rules

This file applies to the whole repository. Read it before building, testing or
cleaning anything.

## Mandatory: clean caches after every build

The Cargo `target/` directory is a cache and grows quickly (it has exceeded
6 GB in this repository). Never leave it or other generated caches behind.

After finishing **any** build, test, clippy, E2E, smoke or benchmark command,
run the cleanup before reporting the task as done:

```bash
npm run clean          # equivalent: scripts/clean.sh
```

This removes:

- `target/` and `src-tauri/target/`
- `test-results/`, `playwright-report/`, `blob-report/`, `.playwright*`, `coverage/`
- `.vite/`, `node_modules/.vite/`, `node_modules/.cache/`
- `__pycache__/`, `.pytest_cache/`, `.mypy_cache/`, `.ruff_cache/`

Rules:

1. If the next step needs a produced binary or report, use it first, then clean.
2. Keep `node_modules/` and `dist/` by default. Use `scripts/clean.sh --dist`
   when `dist/` is no longer needed, and `scripts/clean.sh --all` only when
   removing `node_modules/` is intended.
3. Never clean while a build or test is running. Check with
   `pgrep -af 'cargo|vite|build_guard'` and respect `.opsterm-build.lock`.
4. Verify the cleanup succeeded: `du -sh target dist 2>/dev/null` should report
   nothing.
5. Never commit generated artifacts or caches.

## Build/test commands

Use the guarded entry points so the RAM watchdog stays active:

```bash
npm run build          # frontend
npm run build:egui     # egui client
npm run build:native   # GTK client
npm run test:rust      # Rust tests
npm test               # frontend unit tests
npm run e2e            # Playwright
npm run test:build-guard
```

Raw `cargo` commands must be wrapped as
`python3 scripts/build_guard.py -- cargo <args>`.

The security and process-spawning ground rules in [`CONTRIBUTING.md`](CONTRIBUTING.md)
still apply.
