// SPDX-License-Identifier: GPL-3.0-only

//! The system keyring (MET-005) on Android (D-16): not reached yet (the
//! Android Keystore needs Java). Keys stay in the settings, as with the
//! keyring turned off.

use nikomiru_core::secrets::SecretStore;

const UNAVAILABLE: &str = "no system keyring on Android yet";

/// A keyring that is never there.
#[derive(Debug, Default)]
pub struct SystemKeyring;

impl SecretStore for SystemKeyring {
    fn names(&self) -> Result<Vec<String>, String> {
        Err(UNAVAILABLE.into())
    }

    fn get(&self, _name: &str) -> Result<Option<String>, String> {
        Err(UNAVAILABLE.into())
    }

    fn set(&self, _name: &str, _value: &str) -> Result<(), String> {
        Err(UNAVAILABLE.into())
    }

    fn remove(&self, _name: &str) -> Result<(), String> {
        Err(UNAVAILABLE.into())
    }
}
