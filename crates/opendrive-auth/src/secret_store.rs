//! Secret storage trait and in-memory implementation for credentials and tokens.

use crate::Result;
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

pub trait SecretStore: Send + Sync {
    fn store_refresh_token(&self, account_id: Uuid, token: &str) -> Result<()>;
    fn load_refresh_token(&self, account_id: Uuid) -> Result<Option<String>>;
    fn delete_refresh_token(&self, account_id: Uuid) -> Result<()>;
    fn store_client_secret(&self, cred_id: Uuid, secret: &str) -> Result<()>;
    fn load_client_secret(&self, cred_id: Uuid) -> Result<Option<String>>;
    fn store_service_account_json(&self, account_id: Uuid, json: &str) -> Result<()>;
    fn load_service_account_json(&self, account_id: Uuid) -> Result<Option<String>>;
    fn delete_service_account_json(&self, account_id: Uuid) -> Result<()>;

    fn delete_account_secrets(&self, account_id: Uuid) {
        let _ = self.delete_refresh_token(account_id);
        let _ = self.delete_service_account_json(account_id);
    }
}

#[derive(Debug, Default)]
pub struct MemorySecretStore {
    entries: Mutex<HashMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn set(&self, key: String, value: &str) -> Result<()> {
        self.entries
            .lock()
            .expect("secret store lock")
            .insert(key, value.to_string());
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .entries
            .lock()
            .expect("secret store lock")
            .get(key)
            .cloned())
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.entries.lock().expect("secret store lock").remove(key);
        Ok(())
    }
}

impl SecretStore for MemorySecretStore {
    fn store_refresh_token(&self, account_id: Uuid, token: &str) -> Result<()> {
        self.set(format!("acct/{account_id}/refresh_token"), token)
    }

    fn load_refresh_token(&self, account_id: Uuid) -> Result<Option<String>> {
        self.get(&format!("acct/{account_id}/refresh_token"))
    }

    fn delete_refresh_token(&self, account_id: Uuid) -> Result<()> {
        self.delete(&format!("acct/{account_id}/refresh_token"))
    }

    fn store_client_secret(&self, cred_id: Uuid, secret: &str) -> Result<()> {
        self.set(format!("cred/{cred_id}/secret"), secret)
    }

    fn load_client_secret(&self, cred_id: Uuid) -> Result<Option<String>> {
        self.get(&format!("cred/{cred_id}/secret"))
    }

    fn store_service_account_json(&self, account_id: Uuid, json: &str) -> Result<()> {
        self.set(format!("acct/{account_id}/service_account_json"), json)
    }

    fn load_service_account_json(&self, account_id: Uuid) -> Result<Option<String>> {
        self.get(&format!("acct/{account_id}/service_account_json"))
    }

    fn delete_service_account_json(&self, account_id: Uuid) -> Result<()> {
        self.delete(&format!("acct/{account_id}/service_account_json"))
    }
}
