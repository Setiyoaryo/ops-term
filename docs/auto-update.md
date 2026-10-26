# Signed automatic updates from GitHub Releases

The egui client checks this stable HTTPS feed by default:

```text
https://github.com/Setiyoaryo/ops-term/releases/latest/download/manifest.json
```

Each published stable release contains the Linux x86_64 binary, SHA-256 file,
Ed25519 signature and `manifest.json`. The manifest references version-specific
GitHub asset URLs; the `latest` URL selects the newest stable manifest.

## App behavior

The app checks about three seconds after startup when automatic checking is
enabled. **Help → Check for updates** checks immediately. An available release
is downloaded and installed only when requested; existing terminal sessions
continue until the user chooses to restart.

A public HTTPS feed only offers a version newer than the current binary.
Never overwrite or reuse a published stable tag. Bump the egui version for
each public release. Same-version rebuilds remain available for explicitly
configured local `file://` development feeds.

The manifest URL resolves in this order:

1. Nonempty `OPSTERM_UPDATE_URL` override.
2. A nonempty `manifest_url` in `~/.config/org.opsterm.app/update.json`.
3. The GitHub feed above.

The `auto_check` preference is preserved, including when a config omits other
fields. To point an existing local-only installation at GitHub, set its URL in
the update dialog to the HTTPS feed above. A custom/local URL is not silently
removed by the binary. The developer installation is migrated as part of this
initial public release setup.

## Signature and installation protections

- Only `egui/update-pubkey.txt` is public. The private Ed25519 key lives outside
  the source tree and in an encrypted GitHub repository secret.
- The signature covers version, platform, size, SHA-256 and asset download URL.
- HTTPS redirects remain HTTPS. Plain HTTP requires the explicit local-test
  flag `OPSTERM_UPDATE_ALLOW_HTTP=1`.
- Signature, download size, SHA-256 and ELF header are checked before replacing
  the executable. A rollback copy is kept as `.opsterm-egui.old` beside it.
- Symlink executables are refused. Install the standalone binary into a
  user-writable directory such as `~/.local/bin`.
- Root-owned/package-managed installations must use their package manager.
  Flatpak is a separate distribution/update path and is not required here.

## Publish the next stable release

1. Bump `version` in `egui/Cargo.toml` (for example `0.1.2`).
2. Update the lockfile through guarded Cargo, then commit to `main`:
   `python3 scripts/build_guard.py -- cargo update -p opsterm-egui --offline`.
3. Optionally add `docs/releases/v0.1.2.md` for release notes.
4. Push the commit to `main`, then push a matching `v0.1.2` Git tag.

The **Release egui** workflow runs on a standard `ubuntu-24.04` GitHub-hosted
runner. It validates the stable tag against Cargo, verifies that its commit is
on `main`, runs core/egui tests and Clippy, builds and signs assets, and checks
the manifest with the release binary. It uploads a draft, re-downloads and
compares every asset, then publishes it as latest. Failed/incomplete uploads
remain drafts and do not replace the live feed. Drafts can be resumed on rerun;
published assets are never overwritten. Publishing is serialized and a release
older than the current latest cannot move the update feed backwards. The existing laptop runner is
not reassigned.

Signing material is written to mode-restricted temporary files, not passed as
raw CLI arguments or environment values. The scoped publishing token uses a
temporary GitHub CLI credential file. Private temporary material and build
caches are removed in the final workflow step. Pull-request CI runs on fresh
hosted VMs and never receives the signing key.

The required repository secret is `OPSTERM_UPDATE_SIGNING_KEY`, containing the
existing PEM private key. Keep a secure backup outside the repository. Rotating
the key requires a migration plan because existing installations pin the old
public key.

## Local packaging and headless verification

```bash
npm run package:egui:local
```

That creates a signed local manifest in ignored `release/`. Override the feed
with its file URL for development. For hosted packaging, use:

```bash
npm run package:egui -- --tag v0.1.2 --asset-base-url https://github.com/Setiyoaryo/ops-term/releases/download/v0.1.2
```

The tag must match the current egui Cargo version and use stable `vX.Y.Z`
format. Packaging checks that the private key matches the committed public key.

The application supports `--check-update` and `--install-update` without a GUI.
`--install-update` replaces the executable that invokes it; test it using a
copy in a disposable directory. `curl` and CA certificates are needed for HTTPS.
Only egui is covered; updates download the complete binary, not deltas.

## Hosting

Standard GitHub-hosted Actions runners are free for public repositories:
[GitHub billing documentation](https://docs.github.com/en/actions/concepts/billing-and-usage).
No dedicated server, paid runner or Flatpak remote is needed for this setup.
