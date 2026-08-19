//! DB-key management. Production path: OS credential store via `keyring`
//! (Windows Credential Manager / macOS Keychain / Secret Service).
//! Dev/test path: the POS_DB_KEY env var, so CI and laptops need no vault.

use keyring::Entry;

const SERVICE: &str = "pos-terminal";
const USER: &str = "sqlcipher-db-key";

pub fn get_or_create() -> Result<String, crate::DbError> {
    if let Ok(key) = std::env::var("POS_DB_KEY") {
        if !key.is_empty() {
            return Ok(key);
        }
    }
    let entry = Entry::new(SERVICE, USER)?;
    match entry.get_password() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => {
            let key = uuid::Uuid::new_v4().simple().to_string(); // 128-bit random
            entry.set_password(&key)?;
            Ok(key)
        }
        Err(e) => Err(e.into()),
    }
}
