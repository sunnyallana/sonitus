//! Integration test: full crypto round trip from passphrase → DB → field encryption → DB → passphrase.

use sonitus_core::crypto::{VaultKey, encrypt_field, decrypt_field, types::SourceCredential};
use sonitus_core::crypto::kdf::SALT_LEN;

#[test]
fn passphrase_to_field_round_trip() {
    let passphrase = "correct horse battery staple";
    let salt = [9u8; SALT_LEN];

    let key1 = VaultKey::derive(passphrase, &salt).unwrap();
    let plaintext = b"ya29.a0AfH-DRIVE-OAUTH-ACCESS";
    let ciphertext = encrypt_field(&key1, plaintext).unwrap();

    // Simulate restart: derive the key again from the same passphrase + salt.
    let key2 = VaultKey::derive(passphrase, &salt).unwrap();
    let recovered = decrypt_field(&key2, &ciphertext).unwrap();

    assert_eq!(plaintext.as_ref(), recovered.as_slice());
}

#[test]
fn wrong_passphrase_fails_to_decrypt() {
    let salt = [9u8; SALT_LEN];
    let key = VaultKey::derive("correct horse", &salt).unwrap();
    let ct = encrypt_field(&key, b"secret").unwrap();
    let wrong = VaultKey::derive("wrong horse", &salt).unwrap();
    assert!(decrypt_field(&wrong, &ct).is_err());
}

#[test]
fn source_credential_roundtrip_through_encryption() {
    let salt = [42u8; SALT_LEN];
    let key = VaultKey::derive("hunter2", &salt).unwrap();

    let creds = SourceCredential {
        kind: "google_drive".into(),
        primary: "ya29.access-token".into(),
        secondary: Some("1//refresh-token".into()),
        expires_at: Some(1_700_000_000),
    };
    let pt = creds.to_plaintext();
    let ct = encrypt_field(&key, &pt).unwrap();
    let pt2 = decrypt_field(&key, &ct).unwrap();
    let recovered = SourceCredential::from_plaintext(&pt2).unwrap();

    assert_eq!(recovered.kind, creds.kind);
    assert_eq!(recovered.primary, creds.primary);
    assert_eq!(recovered.secondary, creds.secondary);
    assert_eq!(recovered.expires_at, creds.expires_at);
}
