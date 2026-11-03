// SPDX-License-Identifier: GPL-3.0-only

//! The keys in the system keyring (MET-005), on Windows: not yet; the
//! Credential Manager would hold them (spike S6). Keys stay in `config.toml`.

use nikomiru_core::secrets::SecretStore;

const NONE: &str = "no keyring on Windows yet";

/// Refuses everything, so the keys stay in the file.
#[derive(Debug, Default)]
pub struct SystemKeyring;

impl SecretStore for SystemKeyring {
    fn names(&self) -> Result<Vec<String>, String> {
        Err(NONE.into())
    }

    fn get(&self, _name: &str) -> Result<Option<String>, String> {
        Err(NONE.into())
    }

    fn set(&self, _name: &str, _value: &str) -> Result<(), String> {
        Err(NONE.into())
    }

    fn remove(&self, _name: &str) -> Result<(), String> {
        Err(NONE.into())
    }
}
