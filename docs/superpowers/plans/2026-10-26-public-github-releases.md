# Public GitHub releases and automatic updates

Goal: publish the current MIT-licensed source as Setiyoaryo/ops-term, ship a signed Linux x86_64 egui release, and make future tagged releases available automatically through the app.

Decision: GitHub Releases hosts the versioned binary, checksum, Ed25519 signature and manifest. The stable feed is https://github.com/Setiyoaryo/ops-term/releases/latest/download/manifest.json. Standard GitHub-hosted runners are free for public repos and work when the developer laptop is off. The existing repository-scoped nocify laptop runner is left untouched. Flatpak is deferred because its package-manager update lifecycle would add another distribution path.

- [x] Audit/sanitize the initial public snapshot, exclude credentials and generated output, initialize Git and publish public main.
- [x] Add default update feed, preserve env/custom-feed overrides, enforce incremented public versions while retaining same-version file:// development updates; prove defaults and policy with regression tests.
- [x] Release egui 0.1.1; validate tag/package version before signing. Store private signing key as a repository secret using stdin. Workflows materialize key/token only into restrictive temporary files, never raw argv/env or source. Keep the pinned public key compatible with existing installations.
- [x] CI uses standard hosted runners, read-only credentials, guarded builds, isolated test data and cleanup after reports/binaries are used. Release is trusted-tag-only and uses least-privilege contents:write; publish draft only after signed asset smoke validation.
- [x] Run review/tests and real published update check/install on a disposable copy; switch the local app's existing file feed to the verified HTTPS feed, preserving its disabled/enabled preference. Install the verified version without stopping active sessions and remove caches.

The first release is published only after code audit, signature verification, workflow validation and build checks. Later releases require bumping egui/Cargo.toml, updating the lockfile via Cargo, committing to main and pushing a matching vX.Y.Z tag. Same-version public releases are not reused.
