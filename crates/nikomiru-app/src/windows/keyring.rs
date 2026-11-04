// SPDX-License-Identifier: GPL-3.0-only

//! The keys in the system keyring (MET-005), on Windows (ADR 0009): generic
//! credentials of the Credential Manager, named `nikomiru:<key>`, kept for
//! this user on this computer.

use nikomiru_core::secrets::SecretStore;
use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredEnumerateW,
    CredFree, CredReadW, CredWriteW,
};

use crate::platform::{from_wide, wide};

/// The prefix of this application's credentials.
const PREFIX: &str = "nikomiru:";

/// The Credential Manager of this user.
#[derive(Debug, Default)]
pub struct SystemKeyring;

fn last_error(what: &str) -> String {
    // SAFETY: reads this thread's last error, nothing else.
    let code = unsafe { GetLastError() };
    format!("{what}: Windows error {code}")
}

fn not_found() -> bool {
    // SAFETY: as in `last_error`.
    unsafe { GetLastError() == ERROR_NOT_FOUND }
}

/// The name of a credential, from its NUL-terminated target.
///
/// # Safety
/// `target` must point to a NUL-terminated UTF-16 string.
unsafe fn target_name(target: *const u16) -> String {
    let mut len = 0;
    // SAFETY: the caller's promise: a NUL ends the string.
    while unsafe { *target.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: the `len` units before the NUL were just read.
    from_wide(unsafe { std::slice::from_raw_parts(target, len) })
}

impl SecretStore for SystemKeyring {
    fn names(&self) -> Result<Vec<String>, String> {
        let filter = wide(&format!("{PREFIX}*"));
        let mut count = 0u32;
        let mut list: *mut *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: the filter is NUL-terminated; on success Windows gives a
        // list of `count` credentials, freed below with `CredFree`.
        if unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut list) } == 0 {
            return if not_found() {
                Ok(Vec::new())
            } else {
                Err(last_error("CredEnumerate"))
            };
        }
        let mut names = Vec::new();
        for i in 0..count as usize {
            // SAFETY: `list` holds `count` valid credential pointers, each
            // with a NUL-terminated target name.
            let name = unsafe { target_name((**list.add(i)).TargetName) };
            names.extend(name.strip_prefix(PREFIX).map(str::to_string));
        }
        // SAFETY: `list` came from `CredEnumerateW` and is freed once.
        unsafe { CredFree(list.cast()) };
        Ok(names)
    }

    fn get(&self, name: &str) -> Result<Option<String>, String> {
        let target = wide(&format!("{PREFIX}{name}"));
        let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: the target is NUL-terminated; on success the credential is
        // Windows's, freed below.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
            return if not_found() {
                Ok(None)
            } else {
                Err(last_error("CredRead"))
            };
        }
        // SAFETY: the credential is valid until `CredFree`; its blob has the
        // size it says.
        let value = unsafe {
            let c = &*credential;
            let blob = if c.CredentialBlob.is_null() {
                &[][..]
            } else {
                std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
            };
            String::from_utf8_lossy(blob).into_owned()
        };
        // SAFETY: freed once, after its last use.
        unsafe { CredFree(credential.cast()) };
        Ok(Some(value))
    }

    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        let mut target = wide(&format!("{PREFIX}{name}"));
        let mut user = wide("nikomiru");
        let mut blob = value.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: u32::try_from(blob.len()).map_err(|e| e.to_string())?,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: user.as_mut_ptr(),
            // SAFETY: all-zero is a valid value for the remaining fields
            // (no flags, comment, attributes or alias).
            ..unsafe { std::mem::zeroed() }
        };
        // SAFETY: every pointer in `credential` points into a buffer that
        // lives until the call returns.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(last_error("CredWrite"));
        }
        Ok(())
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        let target = wide(&format!("{PREFIX}{name}"));
        // SAFETY: the target is NUL-terminated.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 && !not_found() {
            return Err(last_error("CredDelete"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-MET-005 on Windows: a key written, read, listed and removed
    #[test]
    fn a_key_goes_in_and_out_of_the_credential_manager() {
        let store = SystemKeyring;
        let name = format!("test-{}", std::process::id());
        store.set(&name, "s3cret ✓").unwrap();
        assert_eq!(store.get(&name).unwrap().as_deref(), Some("s3cret ✓"));
        assert!(store.names().unwrap().contains(&name));
        store.remove(&name).unwrap();
        assert_eq!(store.get(&name).unwrap(), None);
        store.remove(&name).unwrap();
    }
}
