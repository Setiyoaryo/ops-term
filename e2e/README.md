# End-to-end tests (Playwright)

These tests drive the **real frontend bundle** (React + xterm.js + the typed
IPC layer) in Chromium, running the real user journeys:

- open a local shell and execute a command;
- two sessions stay isolated (no cross-session output);
- Quick Connect opens an SSH tab;
- a host that looks like an option is rejected **without** opening a session
  (security acceptance case);
- closing a tab reaps the session;
- Ctrl+Shift+R reconnects.

## Scope boundary (important)

Playwright cannot attach to Tauri's WebKitGTK WebView (the WebView speaks a
different automation protocol from Playwright's bundled browsers). To exercise
the real UI end-to-end, the tests inject `e2e/tauri-mock.js` before the bundle
loads. That file implements the same `window.__TAURI_INTERNALS__` contract and
event-plugin protocol as Tauri, backed by a deterministic **simulated PTY**.

So these tests validate the frontend and the IPC contract, **not** the Rust
providers. The real PTY / OpenSSH behaviour is covered by the Rust integration
tests, which spawn genuine processes:

```bash
cd src-tauri
cargo test                                   # real local PTY round-trip + resize
cargo test --test openssh_pty -- --ignored   # real OpenSSH to a reachable host
```

## Run

```bash
npx playwright install chromium   # one-time
npm run e2e
```

The Playwright `webServer` builds the app and serves `dist/` on
`http://localhost:4173` before running the tests.
