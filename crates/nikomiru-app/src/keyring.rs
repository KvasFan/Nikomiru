// SPDX-License-Identifier: GPL-3.0-only

//! The desktop's keyring over the Secret Service (MET-005): GNOME Keyring,
//! KWallet and the like. Each call opens its own connection: there are few of
//! them, and only for those who turned the keyring on.

use std::collections::HashMap;

use nikomiru_core::secrets::SecretStore;
use secret_service::blocking::{Collection, Item, SecretService};
use secret_service::{EncryptionType, Error};

const APPLICATION: &str = "io.github.kvasfan.Nikomiru";

fn text(e: Error) -> String {
    e.to_string()
}

/// The keys of this application in the Secret Service of the session bus.
#[derive(Debug, Default)]
pub struct SystemKeyring;

impl SystemKeyring {
    fn with<T>(&self, act: impl FnOnce(&Collection<'_>) -> Result<T, Error>) -> Result<T, String> {
        let service = SecretService::connect(EncryptionType::Dh).map_err(text)?;
        let collection = service
            .get_default_collection()
            .or_else(|_| service.get_any_collection())
            .map_err(text)?;
        collection.ensure_unlocked().map_err(text)?;
        act(&collection).map_err(text)
    }
}

fn items<'a>(c: &'a Collection<'_>, name: &str) -> Result<Vec<Item<'a>>, Error> {
    c.search_items(HashMap::from([
        ("application", APPLICATION),
        ("name", name),
    ]))
}

impl SecretStore for SystemKeyring {
    fn names(&self) -> Result<Vec<String>, String> {
        self.with(|c| {
            let mut names = Vec::new();
            for item in c.search_items(HashMap::from([("application", APPLICATION)]))? {
                names.extend(item.get_attributes()?.remove("name"));
            }
            Ok(names)
        })
    }

    fn get(&self, name: &str) -> Result<Option<String>, String> {
        self.with(|c| {
            let Some(item) = items(c, name)?.into_iter().next() else {
                return Ok(None);
            };
            Ok(Some(
                String::from_utf8_lossy(&item.get_secret()?).into_owned(),
            ))
        })
    }

    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        self.with(|c| {
            c.create_item(
                &format!("Nikomiru: {name}"),
                HashMap::from([("application", APPLICATION), ("name", name)]),
                value.as_bytes(),
                true,
                "text/plain",
            )
            .map(|_| ())
        })
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        self.with(|c| {
            for item in items(c, name)? {
                item.delete()?;
            }
            Ok(())
        })
    }
}
