//! The xAI API key, kept in the login Keychain under this app's own entry.
//! The key is never logged.

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

const ACCOUNT: &str = "xai-api-key";
/// errSecItemNotFound
const NOT_FOUND: i32 = -25300;

pub fn load(service: &str) -> Option<String> {
    match get_generic_password(service, ACCOUNT) {
        Ok(key) => String::from_utf8(key).ok(),
        Err(error) => {
            if error.code() != NOT_FOUND {
                tracing::warn!(code = error.code(), "the saved API key could not be read");
            }
            None
        }
    }
}

pub fn save(service: &str, key: &str) -> Result<(), String> {
    set_generic_password(service, ACCOUNT, key.as_bytes()).map_err(|error| error.to_string())
}

pub fn forget(service: &str) -> Result<(), String> {
    match delete_generic_password(service, ACCOUNT) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == NOT_FOUND => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}
