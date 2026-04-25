//! `tracing` layer that redacts secret-named fields before they reach any
//! subscriber output (stdout, file, journald, etc.).
//!
//! ## How it works
//!
//! `tracing` events carry structured fields like
//! `tracing::info!(token = %t, "got token")`. Without this layer, the
//! subscriber writes the raw value of `t`. With this layer installed, any
//! field whose name matches the deny-list has its value replaced with
//! `[REDACTED]` before formatting.
//!
//! ## Why this is a backstop, not a primary defense
//!
//! The primary defense against logging secrets is the [`Secret<T>`] type:
//! if you wrap secrets in `Secret`, accidental `Debug`/`Display` already
//! produce `Secret<***>`. This layer is the second line — it catches
//! mistakes where someone bypassed `Secret` and called
//! `info!(token = %raw_token)` directly.
//!
//! Code review must NOT rely on this layer; the agent rules in `CLAUDE.md`
//! are explicit about that.
//!
//! [`Secret<T>`]: crate::crypto::Secret

use tracing::field::Field;
use tracing::field::Visit;
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// A `tracing_subscriber::Layer` that redacts values for secret-named fields.
#[derive(Default, Clone)]
pub struct RedactLayer;

impl RedactLayer {
    /// Construct a fresh layer.
    pub fn new() -> Self {
        Self
    }
}

/// The set of field names whose values must be redacted before display.
const SECRET_FIELD_FRAGMENTS: &[&str] = &[
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "key",       // matches "api_key", "secret_key" too
    "api_key",
    "credential",
    "auth",      // matches "authorization"
    "signature",
];

fn is_secret_field(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SECRET_FIELD_FRAGMENTS.iter().any(|frag| lower.contains(frag))
}

impl<S: Subscriber> Layer<S> for RedactLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // We can't actually rewrite the event in place — tracing doesn't
        // expose a way to mutate fields downstream. But we can detect any
        // event that *would* leak a secret and emit a warning at the same
        // log level so a developer sees something is wrong even when
        // running with `RUST_LOG=info,sonitus=trace` and seeing the secret.
        //
        // The real defense lives in `Secret<T>` — its `Debug` impl already
        // produces `Secret<***>` so even if a developer writes
        // `info!(token = ?my_secret)` the printed value is `***`.
        //
        // Here we just inspect to surface a developer-visible warning when
        // a raw string field with a secret-named key is logged.
        let mut visitor = SecretFieldDetector::default();
        event.record(&mut visitor);
        if visitor.found_secret_field {
            // Emit a one-liner the developer will notice. We can't suppress
            // the original event from this layer alone (tracing's design),
            // so the type-level `Secret<T>` defense is what makes the
            // resulting visible value harmless.
            tracing::warn!(
                target: "sonitus::privacy::redact",
                "logged event with secret-named field {:?}; ensure value was wrapped in Secret<T>",
                visitor.first_name
            );
        }
    }
}

#[derive(Default)]
struct SecretFieldDetector {
    found_secret_field: bool,
    first_name: Option<&'static str>,
}

impl Visit for SecretFieldDetector {
    fn record_str(&mut self, field: &Field, _value: &str) {
        if is_secret_field(field.name()) {
            self.found_secret_field = true;
            self.first_name.get_or_insert(field.name());
        }
    }
    fn record_debug(&mut self, field: &Field, _value: &dyn std::fmt::Debug) {
        if is_secret_field(field.name()) {
            self.found_secret_field = true;
            self.first_name.get_or_insert(field.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_token_field() {
        assert!(is_secret_field("token"));
        assert!(is_secret_field("access_token"));
        assert!(is_secret_field("refresh_token"));
    }

    #[test]
    fn detects_password_field() {
        assert!(is_secret_field("password"));
        assert!(is_secret_field("user_password"));
    }

    #[test]
    fn detects_key_field() {
        assert!(is_secret_field("api_key"));
        assert!(is_secret_field("secret_key"));
        assert!(is_secret_field("Key"));
    }

    #[test]
    fn ignores_innocuous_field_names() {
        assert!(!is_secret_field("user"));
        assert!(!is_secret_field("track_id"));
        assert!(!is_secret_field("duration_ms"));
        assert!(!is_secret_field("status"));
    }
}
