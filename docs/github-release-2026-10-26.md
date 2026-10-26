# Verified public release: v0.1.1

- Repository: https://github.com/Setiyoaryo/ops-term (public, MIT).
- Release: https://github.com/Setiyoaryo/ops-term/releases/tag/v0.1.1.
- Released source commit: `170ba13e1fbe1018ca0e725fcd2bbb6b13732291`.
- Stable update feed: https://github.com/Setiyoaryo/ops-term/releases/latest/download/manifest.json.
- Platform: Linux x86_64, built on Ubuntu 24.04 with Rust 1.94.1.

Assets: `opsterm-egui-0.1.1-linux-x86_64`, its `.sha256` and `.sig`, and
`manifest.json`. Binary SHA-256:

```text
29415623cd37f180e62531b393315178acf06776d1f571ec0d3db36c23c54b3b
```

## Evidence

[CI](https://github.com/Setiyoaryo/ops-term/actions/runs/40306650096) passed:
frontend unit tests, packaging/publisher tests, build/E2E, Rust format/Clippy,
and 54 core + 85 egui tests.

[Release workflow](https://github.com/Setiyoaryo/ops-term/actions/runs/40306993241)
passed the release-profile Rust suite, Clippy, build, signing, native CLI
manifest verification, upload/download equality checks, publication as latest,
and private-material/build-cache cleanup. Peak sampled build RSS stayed below
the guard limit (about 1.17 GiB during release tests).

The real public HTTPS update path was tested after publication using a
**disposable copy of the older installed binary**:

1. The old client detected v0.1.1 from the public manifest.
2. `--install-update` verified the pinned Ed25519 signature, size and SHA-256,
   downloaded and installed the actual public release binary.
3. The `.opsterm-egui.old` rollback copy matched the original binary hash.
4. The new executable matched the published checksum.
5. With the test URL override removed and an empty config directory, the new
   executable used its built-in GitHub feed and reported v0.1.1 up to date.

The developer installation was replaced atomically with that verified binary;
its previous local-file feed was migrated to GitHub while preserving the
`auto_check` preference. A subsequent check using the installed binary and real
configuration reported up to date. Existing terminal processes were left
running and use the new version after restart.

Local build/test caches were removed. Downloaded release deliverables remain
in ignored `release/`; generated assets and private keys were not committed.
The existing laptop runner was not reassigned. Standard GitHub-hosted runners
handled CI and release, with no Actions artifact/cache uploads configured.

## Next release

Follow [auto-update publishing instructions](auto-update.md): bump the egui
version, update Cargo.lock using Cargo, commit to main, then push a matching
stable `vX.Y.Z` tag. The workflow resumes incomplete drafts, refuses to overwrite
published assets, and does not move the latest feed to an older version.
Automatic checking is enabled by default; installing and restarting remain
explicit user actions so active sessions are not interrupted.
