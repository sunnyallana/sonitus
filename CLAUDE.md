# Sonitus — Claude Code Agent Instructions

This is **Sonitus**, a local-first, encrypted, cross-platform music streaming
application written entirely in Rust. It is a **complete production application**
— not an MVP, not a prototype. No stubs. No TODOs. No placeholders.

When you work on this codebase, treat every file as production code that ships
to real users. Every function must be fully implemented before moving on.

---

## The Five Privacy Guarantees (load-bearing — never violate)

These are not aspirations. They are enforced in code, marketed to users, and
audited by `cargo-deny` and CI. Breaking any of them is a release blocker.

| # | Name              | Enforcement                                                |
|---|-------------------|------------------------------------------------------------|
| 1 | Local-first       | No Sonitus server. SQLite + `.sonitus` file on device.     |
| 2 | Encrypted at rest | XChaCha20-Poly1305, key from Argon2id-derived passphrase.  |
| 3 | Zero telemetry    | `deny.toml` blocks all telemetry crates at build time.     |
| 4 | Credential iso.   | OAuth tokens in encrypted vault. `Zeroize` on drop.        |
| 5 | Auditable         | Every outbound HTTP request logged via `AuditMiddleware`.  |

---

## Hard rules — the user has explicitly asked for these

1. **PRIVACY**: Every `reqwest` request **MUST** go through `AuditMiddleware`.
   Never construct a bare `reqwest::Client`. Always use `ClientBuilder` with
   `.with(AuditMiddleware::new(...))`. The audit log is a contract with users.

2. **SECRETS**: Never store secrets in plaintext. All OAuth tokens, passwords,
   and S3 secret keys **MUST** be encrypted via `crypto::field::encrypt_field()`
   before writing to the database.

3. **MEMORY**: All types holding secret data **MUST** derive `Zeroize` +
   `ZeroizeOnDrop` (or implement `Drop` manually with `zeroize()`). Never
   `clone()` a `Secret<T>` — pass by reference and call `.expose()` only at
   the point of use.

4. **LOGGING**: Never log `Secret<T>` values. The `tracing` redact layer in
   `privacy/redact.rs` catches most cases, but don't rely on it as a backstop —
   review every `tracing::info!` / `debug!` for accidental token leakage.

5. **CONSENT**: Calls to `metadata/musicbrainz.rs` and `metadata/acoustid.rs`
   **MUST** be gated by `consent.is_enabled(Feature::MetadataLookups)`.
   Default is `false`. The consent toggle UI must explain exactly what data
   is sent, to whom, and why.

6. **TELEMETRY**: Never add analytics, crash reporting, or usage tracking.
   After adding any new dependency, run `cargo deny check` — if it fails,
   the dependency does not enter the tree.

7. **TLS**: Always use `rustls-tls` feature of `reqwest`. **Never** use
   `native-tls` (it leaks the system trust store and ties us to OpenSSL).

8. **`unsafe`**: No `unsafe` blocks unless absolutely required for `cpal`,
   WASM audio output, or platform FFI. If `unsafe` is needed, document why
   with a `// SAFETY:` comment that explains the invariant being upheld.

---

## Build & test commands

```bash
# ── Development ──────────────────────────────────────────────────────────────
dx serve --platform desktop          # desktop hot-reload dev server
dx serve --platform web              # web (WASM) dev server
dx serve --platform ios              # iOS simulator
dx serve --platform android          # Android emulator
dx serve --hotpatch                  # Dioxus 0.7 subsecond hot-patching

# ── Quality gates (run all of these before pushing) ──────────────────────────
cargo xfmt                           # cargo fmt --all
cargo xlint                          # cargo clippy -- -D warnings
cargo xtest                          # cargo test --workspace
cargo xdeny                          # cargo deny check
cargo audit                          # security advisories

# ── Release builds ───────────────────────────────────────────────────────────
dx bundle --platform desktop --release
dx bundle --platform web     --release
dx bundle --platform ios     --release
dx bundle --platform android --release

# ── Benchmarks ───────────────────────────────────────────────────────────────
cargo bench -p sonitus-core
```

---

## Code quality standards

- All public items have rustdoc comments with at least a one-line summary.
- All `Error` variants have descriptive `#[error(...)]` messages.
- **No `unwrap()` in library code**. Use `?` and propagate errors. The only
  exceptions are: test code, `main.rs` startup (where panics are intentional),
  and `OnceCell::get().unwrap()` after a verified `set()`.
- **No `panic!()` in library code**. Return `Err(SonitusError::...)`.
- All async functions that touch the network or filesystem **MUST** wrap their
  body in `tokio::time::timeout` with an explicit, configurable deadline.
- All DB queries **MUST** use `sqlx::query!` / `sqlx::query_as!` (compile-time
  checked against `migrations/`). Inline raw SQL is forbidden.

---

## Architecture invariants

- **`sonitus-core`**: zero UI deps. Never imports `dioxus*`. Compiles to all
  targets including `wasm32-unknown-unknown`.
- **`sonitus-meta`**: zero DB deps. Only `toml` + `serde`. Compiles everywhere.
  This crate is the user's data format — keep it stable and well-versioned.
- **`sonitus-ui`**: all Dioxus components. Platform-specific code lives in
  `platform/` behind `#[cfg]`. Never put platform code outside `platform/`.
- **The `.sonitus` file is the user's data**. The SQLite DB is a derived,
  rebuildable cache. Design accordingly: data flows .sonitus → DB, never DB → .sonitus
  except via explicit user-triggered "save" operations.
- **The player runs on OS threads, not tokio tasks**. Real-time audio cannot
  tolerate executor scheduling latency. The decode thread and output thread
  are dedicated `std::thread::spawn` workers.
- **The UI subscribes to `PlayerEvent` and `LibraryEvent`** via `crossbeam-channel`
  and `dioxus::Signal`. Never poll. Never block the UI thread on player or DB.

---

## Adding a new dependency

1. Justify it: is it strictly necessary? Can `std` do this?
2. Check its license against `deny.toml` allow-list.
3. Add it to `[workspace.dependencies]` in the root `Cargo.toml` only.
4. Run `cargo deny check` — it must pass.
5. Run `cargo audit` — no new advisories.
6. If it's an HTTP client lib, ensure it routes through `AuditMiddleware`.

## Adding a new outbound HTTP destination

1. Document the destination in the consent toggle (if user-facing).
2. Wire the request through the shared `reqwest_middleware::ClientWithMiddleware`
   built in `sonitus-core::privacy::http_client()`.
3. Verify the audit log records it.
4. Add a test in `tests/privacy/test_audit.rs` that confirms the recording.

---

If you're unsure whether something violates these rules, **ask first**. The
worst outcome is a privacy regression shipping to users; the second-worst is
churning on a refactor that wouldn't have been needed if you'd asked.
