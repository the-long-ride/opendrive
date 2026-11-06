//! Tests for password policy validation and AES-256-GCM encrypted credential vault export/import.

use opendrive_auth::{
    decrypt_vault, encrypt_vault, validate_vault_password, AuthError, ExportedAccount,
    ExportedCredentialSet, VaultPayload,
};

#[test]
fn vault_password_policy_enforces_length_and_four_character_classes() {
    assert!(validate_vault_password("Short1!").is_err());
    assert!(validate_vault_password("Abcdef1!").is_err()); // exact 8 fails (> 8 required)
    assert!(validate_vault_password("alllower1!").is_err());
    assert!(validate_vault_password("ALLUPPER1!").is_err());
    assert!(validate_vault_password("NoDigitsHere!").is_err());
    assert!(validate_vault_password("NoSymbol123").is_err());
    assert!(validate_vault_password("NoSpace Only1").is_err());
    assert!(validate_vault_password("StrongPass9!").is_ok());
}

#[test]
fn encrypt_and_decrypt_vault_roundtrip_and_failure_modes() {
    let payload = VaultPayload {
        exported_at: "2026-11-19T00:00:00Z".into(),
        credentials: vec![ExportedCredentialSet {
            label: "Desktop OAuth".into(),
            client_id: "cid-123".into(),
            client_secret: "secret-xyz".into(),
        }],
        accounts: vec![ExportedAccount {
            nickname: "alice".into(),
            email: "alice@example.com".into(),
            folder_name: "Alice".into(),
            read_only: false,
            enabled: true,
            credential_client_id: Some("cid-123".into()),
            refresh_token: Some("rt-999".into()),
            service_account_json: None,
            avatar_url: Some("https://example.com/a.png".into()),
        }],
    };

    assert!(matches!(
        encrypt_vault(&payload, "weak"),
        Err(AuthError::Vault(_))
    ));
    let encrypted = encrypt_vault(&payload, "StrongPass9!").unwrap();
    assert_eq!(decrypt_vault(&encrypted, "StrongPass9!").unwrap(), payload);
    assert!(matches!(
        decrypt_vault(&encrypted, "WrongPass9!"),
        Err(AuthError::Vault(_))
    ));
    assert!(matches!(
        decrypt_vault(&encrypted, ""),
        Err(AuthError::Vault(_))
    ));

    let mut tampered: serde_json::Value = serde_json::from_slice(&encrypted).unwrap();
    tampered["version"] = serde_json::json!(99);
    let bad_ver = serde_json::to_vec(&tampered).unwrap();
    assert!(matches!(
        decrypt_vault(&bad_ver, "StrongPass9!"),
        Err(AuthError::Vault(_))
    ));

    let mut short_nonce: serde_json::Value = serde_json::from_slice(&encrypted).unwrap();
    short_nonce["nonce_b64"] = serde_json::json!("AAAA");
    assert!(matches!(
        decrypt_vault(&serde_json::to_vec(&short_nonce).unwrap(), "StrongPass9!"),
        Err(AuthError::Vault(_))
    ));
}
