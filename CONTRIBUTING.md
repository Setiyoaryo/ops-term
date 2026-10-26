# Contributing

## Ground rules

1. **No fake completion.** A feature is only "Implemented" when a real
   end-to-end test demonstrates it. Do not merge stubs that look like
   terminal output or that fake remote behaviour.
2. **Never weaken security.** No host-key bypass flags, no `sshpass`, no
   secrets in argv, environment, logs or SQLite.
3. **No shell strings.** Spawn processes with validated argv arrays only.
4. **Follow the milestone order.** Do not start M2+ work while M1 exit gates
   are unproven (PRD §3.1).

## Development

```bash
npm install
npm run tauri:dev
```

## Before you push

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd .. && npm run build && npm test
```

CI runs the same set (see `.github/workflows/ci.yml`).

## After every build or test (mandatory)

Build caches are not kept in the working tree. Once you no longer need the
produced binary or report, clean them:

```bash
npm run clean            # target/, test output, tooling caches (keeps node_modules and dist)
npm run clean:all        # also removes node_modules and dist
```

Do not leave `target/` (multiple GB) behind, and never commit build output.
See [`AGENTS.md`](AGENTS.md) for the full rule.

## Where things live

- Backend behaviour: `src-tauri/src/{providers,session_supervisor,security}`.
- IPC surface: `src-tauri/src/commands`.
- UI: `src/components`, `src/features`, `src/state`.
- New architectural decisions: add a numbered record in `docs/adr/`.
- Feature status changes: update `FEATURE_STATUS.md` in the same PR.

## Commit style

Concise imperative subject (`add serial device enumeration`), with a body
explaining the "why" and the test evidence when the change is non-trivial.
