//! OS keyring storage for refresh tokens and OAuth client secrets.

use crate::{AuthError, Result};
use uuid::Uuid;

const SERVICE: &str = "opendrive";

fn entry(user: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, user).map_err(|e| AuthError::Keychain(e.to_string()))
}

pub fn store_refresh_token(account_id: Uuid, token: &str) -> Result<()> {
    entry(&format!("acct/{account_id}/refresh_token"))?
        .set_password(token)
        .map_err(|e| AuthError::Keychain(e.to_string()))
}

pub fn load_refresh_token(account_id: Uuid) -> Result<Option<String>> {
    match entry(&format!("acct/{account_id}/refresh_token"))?.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AuthError::Keychain(e.to_string())),
    }
}

pub fn delete_refresh_token(account_id: Uuid) -> Result<()> {
    match entry(&format!("acct/{account_id}/refresh_token"))?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::Keychain(e.to_string())),
    }
}

pub fn store_client_secret(cred_id: Uuid, secret: &str) -> Result<()> {
    entry(&format!("cred/{cred_id}/secret"))?
        .set_password(secret)
        .map_err(|e| AuthError::Keychain(e.to_string()))
}

pub fn load_client_secret(cred_id: Uuid) -> Result<Option<String>> {
    match entry(&format!("cred/{cred_id}/secret"))?.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AuthError::Keychain(e.to_string())),
    }
}

pub fn store_service_account_json(account_id: Uuid, json: &str) -> Result<()> {
    let compact = serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| serde_json::to_string(&v).ok())
        .unwrap_or_else(|| json.trim().to_string());
    entry(&format!("acct/{account_id}/service_account_json"))?
        .set_secret(compact.as_bytes())
        .map_err(|e| AuthError::Keychain(e.to_string()))
}

pub fn load_service_account_json(account_id: Uuid) -> Result<Option<String>> {
    let e = entry(&format!("acct/{account_id}/service_account_json"))?;
    match e.get_secret() {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|err| AuthError::Keychain(err.to_string())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => match e.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(AuthError::Keychain(err.to_string())),
        },
    }
}

pub fn delete_service_account_json(account_id: Uuid) -> Result<()> {
    match entry(&format!("acct/{account_id}/service_account_json"))?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::Keychain(e.to_string())),
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeyringStore;

impl crate::SecretStore for OsKeyringStore {
    fn store_refresh_token(&self, account_id: Uuid, token: &str) -> Result<()> {
        store_refresh_token(account_id, token)
    }

    fn load_refresh_token(&self, account_id: Uuid) -> Result<Option<String>> {
        load_refresh_token(account_id)
    }

    fn delete_refresh_token(&self, account_id: Uuid) -> Result<()> {
        delete_refresh_token(account_id)
    }

    fn store_client_secret(&self, cred_id: Uuid, secret: &str) -> Result<()> {
        store_client_secret(cred_id, secret)
    }

    fn load_client_secret(&self, cred_id: Uuid) -> Result<Option<String>> {
        load_client_secret(cred_id)
    }

    fn store_service_account_json(&self, account_id: Uuid, json: &str) -> Result<()> {
        store_service_account_json(account_id, json)
    }

    fn load_service_account_json(&self, account_id: Uuid) -> Result<Option<String>> {
        load_service_account_json(account_id)
    }

    fn delete_service_account_json(&self, account_id: Uuid) -> Result<()> {
        delete_service_account_json(account_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "exercises live OS credential manager"]
    fn os_keyring_live_roundtrip_and_cleanup() {
        let acct = Uuid::new_v4();
        let cred = Uuid::new_v4();
        store_client_secret(cred, "GOCSPX-live-test").unwrap();
        assert_eq!(
            load_client_secret(cred).unwrap().as_deref(),
            Some("GOCSPX-live-test")
        );
        store_refresh_token(acct, "1//live-test-rt").unwrap();
        assert_eq!(
            load_refresh_token(acct).unwrap().as_deref(),
            Some("1//live-test-rt")
        );
        let sa = format!(r#"{{"type":"service_account","k":"{}"}}"#, "x".repeat(1600));
        store_service_account_json(acct, &sa).unwrap();
        let loaded = load_service_account_json(acct).unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&loaded).unwrap(),
            serde_json::from_str::<serde_json::Value>(&sa).unwrap()
        );
        delete_refresh_token(acct).unwrap();
        delete_service_account_json(acct).unwrap();
        entry(&format!("cred/{cred}/secret"))
            .unwrap()
            .delete_credential()
            .unwrap();
    }
}
