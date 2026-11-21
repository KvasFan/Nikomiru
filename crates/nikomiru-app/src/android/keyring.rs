// SPDX-License-Identifier: GPL-3.0-only

//! The keys in the system keyring (MET-005) on Android (D-16, D-85): an AES
//! key made and kept by the Android Keystore, which never lets it out of
//! the system (in the phone's secure hardware where there is one), seals
//! each key; the sealed keys are kept in `keyring.json` in the app's own
//! storage. Without the Keystore's key, the file reads as noise.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use jni::objects::{JByteArray, JObject, JValue};
use jni::{Env, jni_sig, jni_str};
use nikomiru_core::secrets::SecretStore;

use crate::platform::android::with_activity;

/// The Keystore entry of the key that seals the others.
const ALIAS: &str = "nikomiru-keyring";
/// The file of sealed keys, in the app's folder.
const FILE: &str = "keyring.json";
/// `Cipher.ENCRYPT_MODE` and `DECRYPT_MODE`.
const ENCRYPT: i32 = 1;
const DECRYPT: i32 = 2;
/// `KeyProperties.PURPOSE_ENCRYPT | PURPOSE_DECRYPT`.
const PURPOSES: i32 = 3;
/// The length of GCM's tag, in bits.
const TAG_BITS: i32 = 128;

/// One read-change-write of the file at a time.
static FILE_LOCK: Mutex<()> = Mutex::new(());

/// The Android Keystore.
#[derive(Debug, Default)]
pub struct SystemKeyring;

fn path() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(FILE))
        .ok_or_else(|| "no app folder".to_string())
}

/// The sealed keys: name → hex of the nonce and the sealed value.
fn load() -> Result<BTreeMap<String, String>, String> {
    let path = path()?;
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn save(sealed: &BTreeMap<String, String>) -> Result<(), String> {
    let path = path()?;
    let text = serde_json::to_string_pretty(sealed).map_err(|e| e.to_string())?;
    // a new file, then renamed over the old one: never half a file
    let new = path.with_extension("json.new");
    std::fs::write(&new, text)
        .and_then(|()| std::fs::rename(&new, &path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

fn strings<'l>(env: &mut Env<'l>, items: &[&str]) -> jni::errors::Result<JObject<'l>> {
    let array = env.new_object_array(
        i32::try_from(items.len()).unwrap_or(i32::MAX),
        jni_str!("java/lang/String"),
        JObject::null(),
    )?;
    for (i, item) in items.iter().enumerate() {
        let item = env.new_string(item)?;
        array.set_element(env, i, &item)?;
    }
    Ok(array.into())
}

/// The Keystore's key, made the first time.
fn key<'l>(env: &mut Env<'l>) -> jni::errors::Result<JObject<'l>> {
    let provider = env.new_string("AndroidKeyStore")?;
    let store = env
        .call_static_method(
            jni_str!("java/security/KeyStore"),
            jni_str!("getInstance"),
            jni_sig!("(Ljava/lang/String;)Ljava/security/KeyStore;"),
            &[JValue::Object(&provider)],
        )?
        .l()?;
    env.call_method(
        &store,
        jni_str!("load"),
        jni_sig!("(Ljava/security/KeyStore$LoadStoreParameter;)V"),
        &[JValue::Object(&JObject::null())],
    )?;
    let alias = env.new_string(ALIAS)?;
    let key = env
        .call_method(
            &store,
            jni_str!("getKey"),
            jni_sig!("(Ljava/lang/String;[C)Ljava/security/Key;"),
            &[JValue::Object(&alias), JValue::Object(&JObject::null())],
        )?
        .l()?;
    if !key.is_null() {
        return Ok(key);
    }
    let algorithm = env.new_string("AES")?;
    let generator = env
        .call_static_method(
            jni_str!("javax/crypto/KeyGenerator"),
            jni_str!("getInstance"),
            jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;"),
            &[JValue::Object(&algorithm), JValue::Object(&provider)],
        )?
        .l()?;
    let builder = env.new_object(
        jni_str!("android/security/keystore/KeyGenParameterSpec$Builder"),
        jni_sig!("(Ljava/lang/String;I)V"),
        &[JValue::Object(&alias), JValue::Int(PURPOSES)],
    )?;
    let modes = strings(env, &["GCM"])?;
    env.call_method(
        &builder,
        jni_str!("setBlockModes"),
        jni_sig!("([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;"),
        &[JValue::Object(&modes)],
    )?;
    let paddings = strings(env, &["NoPadding"])?;
    env.call_method(
        &builder,
        jni_str!("setEncryptionPaddings"),
        jni_sig!("([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;"),
        &[JValue::Object(&paddings)],
    )?;
    let spec = env
        .call_method(
            &builder,
            jni_str!("build"),
            jni_sig!("()Landroid/security/keystore/KeyGenParameterSpec;"),
            &[],
        )?
        .l()?;
    env.call_method(
        &generator,
        jni_str!("init"),
        jni_sig!("(Ljava/security/spec/AlgorithmParameterSpec;)V"),
        &[JValue::Object(&spec)],
    )?;
    env.call_method(
        &generator,
        jni_str!("generateKey"),
        jni_sig!("()Ljavax/crypto/SecretKey;"),
        &[],
    )?
    .l()
}

fn cipher<'l>(env: &mut Env<'l>) -> jni::errors::Result<JObject<'l>> {
    let name = env.new_string("AES/GCM/NoPadding")?;
    env.call_static_method(
        jni_str!("javax/crypto/Cipher"),
        jni_str!("getInstance"),
        jni_sig!("(Ljava/lang/String;)Ljavax/crypto/Cipher;"),
        &[JValue::Object(&name)],
    )?
    .l()
}

fn bytes(env: &mut Env<'_>, array: JObject<'_>) -> jni::errors::Result<Vec<u8>> {
    let array = env.cast_local::<JByteArray>(array)?;
    env.convert_byte_array(&array)
}

/// The nonce and the sealed value, in hex.
fn seal(value: &str) -> Result<String, String> {
    with_activity(|env, _| {
        let key = key(env)?;
        let cipher = cipher(env)?;
        env.call_method(
            &cipher,
            jni_str!("init"),
            jni_sig!("(ILjava/security/Key;)V"),
            &[JValue::Int(ENCRYPT), JValue::Object(&key)],
        )?;
        let nonce = env
            .call_method(&cipher, jni_str!("getIV"), jni_sig!("()[B"), &[])?
            .l()?;
        let nonce = bytes(env, nonce)?;
        let plain = env.byte_array_from_slice(value.as_bytes())?;
        let sealed = env
            .call_method(
                &cipher,
                jni_str!("doFinal"),
                jni_sig!("([B)[B"),
                &[JValue::Object(&plain)],
            )?
            .l()?;
        let sealed = bytes(env, sealed)?;
        Ok(format!("{}:{}", hex(&nonce), hex(&sealed)))
    })
}

fn open(sealed: &str) -> Result<String, String> {
    let (nonce, value) = sealed
        .split_once(':')
        .and_then(|(n, v)| Some((unhex(n)?, unhex(v)?)))
        .ok_or_else(|| "a sealed key is damaged".to_string())?;
    let plain = with_activity(|env, _| {
        let key = key(env)?;
        let cipher = cipher(env)?;
        let nonce = env.byte_array_from_slice(&nonce)?;
        let spec = env.new_object(
            jni_str!("javax/crypto/spec/GCMParameterSpec"),
            jni_sig!("(I[B)V"),
            &[JValue::Int(TAG_BITS), JValue::Object(&nonce)],
        )?;
        env.call_method(
            &cipher,
            jni_str!("init"),
            jni_sig!("(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V"),
            &[
                JValue::Int(DECRYPT),
                JValue::Object(&key),
                JValue::Object(&spec),
            ],
        )?;
        let value = env.byte_array_from_slice(&value)?;
        let plain = env
            .call_method(
                &cipher,
                jni_str!("doFinal"),
                jni_sig!("([B)[B"),
                &[JValue::Object(&value)],
            )?
            .l()?;
        bytes(env, plain)
    })?;
    String::from_utf8(plain).map_err(|_| "a sealed key is not text".to_string())
}

impl SecretStore for SystemKeyring {
    fn names(&self) -> Result<Vec<String>, String> {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        Ok(load()?.into_keys().collect())
    }

    fn get(&self, name: &str) -> Result<Option<String>, String> {
        let sealed = {
            let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            load()?.get(name).cloned()
        };
        sealed.map(|s| open(&s)).transpose()
    }

    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        let sealed = seal(value)?;
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut all = load()?;
        all.insert(name.to_string(), sealed);
        save(&all)
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut all = load()?;
        if all.remove(name).is_some() {
            save(&all)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_goes_both_ways() {
        let bytes = [0u8, 1, 0xab, 0xff];
        assert_eq!(unhex(&hex(&bytes)).unwrap(), bytes);
        assert!(unhex("abc").is_none());
        assert!(unhex("zz").is_none());
    }
}
