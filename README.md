# Sonitus

> **Your music. Your data. Encrypted, local-first, always yours.**

Sonitus is a complete cross-platform music streaming and library management
application written entirely in Rust. It runs on macOS, Linux, Windows, iOS,
Android, and the web — all from a single Dioxus 0.7 codebase.

Sonitus does not have a server. It does not collect telemetry. It does not
phone home. Your library, your playlists, and your listening history live
on your devices, encrypted, in formats you can read and back up yourself.

---

## The five privacy guarantees

| # | Guarantee         | What it means                                                |
|---|-------------------|--------------------------------------------------------------|
| 1 | **Local-first**   | No Sonitus server. Your library lives on your device.        |
| 2 | **Encrypted**     | SQLite vault sealed with XChaCha20-Poly1305 + Argon2id.      |
| 3 | **Zero telemetry**| No analytics. No crash reporting. Enforced by `cargo-deny`.  |
| 4 | **Credential iso**| OAuth tokens encrypted, zeroed from memory after use.        |
| 5 | **Auditable**     | Every outbound HTTP request logged. You can read it.         |

These are not promises in marketing copy. They are enforced in code:

- `crates/sonitus-core/src/privacy/middleware.rs` records every HTTP request.
- `deny.toml` blocks every known telemetry/analytics crate at compile time.
- `crates/sonitus-core/src/crypto/` is the only place secrets ever exist
  unencrypted, and even there they're wrapped in `Zeroize` types.
- `cargo audit` and `cargo deny check` run in CI on every commit.

---

## Sources

Sonitus does not host your music. It indexes music you already have, wherever
you keep it:

- **Local files** (your computer, an external drive)
- **Google Drive** (OAuth2 PKCE)
- **Amazon S3** (presigned URLs, byte-range streaming)
- **SMB / CIFS** (your home NAS)
- **Generic HTTP** (any directory listing, byte-range capable)
- **Dropbox** (API v2)
- **Microsoft OneDrive** (Graph API)

You can mix and match. A single playlist can pull tracks from your laptop,
your NAS, and Google Drive — Sonitus streams them seamlessly.

---

## Build & run

```bash
# Install the Dioxus CLI:
curl -fsSL https://dioxuslabs.com/install.sh | bash

# Run on desktop (with hot reload):
dx serve --platform desktop

# Bundle a release for your platform:
dx bundle --platform desktop --release
```

See [`CLAUDE.md`](./CLAUDE.md) for full developer documentation, hard rules
for contributors, and the architecture invariants the codebase relies on.

---

## License

MIT — see [`LICENSE`](./LICENSE).

This codebase contains cryptographic software. Audit it. We did, but you
should too. The relevant code is in `crates/sonitus-core/src/crypto/` and
`crates/sonitus-core/src/privacy/`.
