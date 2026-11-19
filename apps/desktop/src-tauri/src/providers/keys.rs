//! API keys, kept in Windows Credential Manager under "Joona". Rust makes
//! every call that needs one; keys never go back to the UI.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

const SERVICE: &str = "Joona";

#[derive(Default)]
pub struct Keys {
    /// Which providers have a key, so routing doesn't ask Windows each time.
    known: Mutex<HashMap<String, bool>>,
}

impl Keys {
    pub fn get(&self, provider: &str) -> anyhow::Result<Option<String>> {
        let key = match entry(provider)?.get_password() {
            Ok(key) => Some(key),
            Err(keyring::Error::NoEntry) => None,
            Err(e) => return Err(e.into()),
        };
        self.remember(provider, key.is_some());
        Ok(key)
    }

    pub fn has(&self, provider: &str) -> bool {
        if let Some(&known) = self.lock().get(provider) {
            return known;
        }
        self.get(provider).is_ok_and(|key| key.is_some())
    }

    pub fn set(&self, provider: &str, key: &str) -> anyhow::Result<()> {
        entry(provider)?.set_password(key)?;
        self.remember(provider, true);
        Ok(())
    }

    pub fn delete(&self, provider: &str) -> anyhow::Result<()> {
        match entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.into()),
        }
        self.remember(provider, false);
        Ok(())
    }

    fn remember(&self, provider: &str, present: bool) {
        self.lock().insert(provider.to_owned(), present);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, bool>> {
        self.known.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn entry(provider: &str) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, provider)
}
