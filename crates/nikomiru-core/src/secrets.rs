// SPDX-License-Identifier: GPL-3.0-only

//! Keys and passwords in the system keyring (MET-005), for those who ask.
//!
//! A key leaves `config.toml` only once the keyring has given it back, and
//! nothing is deleted from the keyring unless it answered at the start: a
//! locked or missing keyring costs a message, never a key. Without
//! `[general] keyring` the store is not touched at all, so no desktop service
//! starts and no password is asked.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;

use crate::Config;

/// How long the start waits for the keyring, which may ask for its password.
pub const ANSWER: Duration = Duration::from_secs(60);

/// Where the keys go. The messages of its errors are shown to the user and
/// never hold a key.
pub trait SecretStore: Send + Sync {
    /// The names of the keys it holds.
    fn names(&self) -> Result<Vec<String>, String>;
    /// The key called `name`.
    fn get(&self, name: &str) -> Result<Option<String>, String>;
    /// Keeps `value` as the key called `name`.
    fn set(&self, name: &str, value: &str) -> Result<(), String>;
    /// Forgets the key called `name`.
    fn remove(&self, name: &str) -> Result<(), String>;
}

/// Writes `value`, and reads it back: a keyring that accepts a key and loses
/// it must not cost the copy in the file.
fn keep(store: &dyn SecretStore, name: &str, value: &str) -> Result<(), String> {
    store.set(name, value)?;
    match store.get(name)? {
        Some(read) if read == value => Ok(()),
        _ => Err(format!("{name} did not come back")),
    }
}

#[derive(Debug, Clone, Copy)]
struct State {
    /// The keyring is in use.
    on: bool,
    /// Every key the keyring holds is in the settings in memory, so a blank
    /// key there means the user cleared it. False after a failed start.
    whole: bool,
}

/// Moves the keys between the settings and the keyring.
pub struct Keeper {
    store: Arc<dyn SecretStore>,
    state: Mutex<State>,
    answer: Duration,
}

impl std::fmt::Debug for Keeper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keeper")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl Keeper {
    /// A keeper over `store`.
    pub fn new(store: Arc<dyn SecretStore>) -> Self {
        Self::with_answer(store, ANSWER)
    }

    fn with_answer(store: Arc<dyn SecretStore>, answer: Duration) -> Self {
        Self {
            store,
            state: Mutex::new(State {
                on: false,
                whole: true,
            }),
            answer,
        }
    }

    fn state(&self) -> State {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_state(&self, state: State) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }

    /// At the start: the blank keys of `config` from the keyring. A key still
    /// in the file wins and moves there, the result telling that the file
    /// can be written without them. An error leaves `config` as it was.
    pub fn open(&self, config: &mut Config) -> Result<bool, String> {
        if !config.general.keyring {
            return Ok(false);
        }
        self.set_state(State {
            on: true,
            whole: false,
        });
        let secrets = config.secrets();
        let store = Arc::clone(&self.store);
        let (sender, receiver) = mpsc::channel();
        // a keyring that hangs must not hold the window back for ever
        std::thread::Builder::new()
            .name("keyring".into())
            .spawn(move || {
                let mut moved = false;
                let read = secrets
                    .into_iter()
                    .map(|(name, value)| {
                        if value.is_empty() {
                            let found = store.get(&name)?;
                            Ok((name, found))
                        } else {
                            keep(&*store, &name, &value)?;
                            moved = true;
                            Ok((name, None))
                        }
                    })
                    .collect::<Result<Vec<_>, String>>();
                let _ = sender.send(read.map(|read| (read, moved)));
            })
            .map_err(|e| e.to_string())?;
        let (read, moved) = receiver
            .recv_timeout(self.answer)
            .map_err(|_| "no answer".to_string())??;
        for (name, value) in config.secrets_mut() {
            let found = read.iter().find(|(n, _)| *n == name);
            if let Some((_, Some(found))) = found {
                value.clone_from(found);
            }
        }
        self.set_state(State {
            on: true,
            whole: true,
        });
        Ok(moved)
    }

    /// What to write to the file for `config`: without its keys once the
    /// keyring holds them, or with them and the reason it does not.
    pub fn before_save(&self, config: &Config) -> (Config, Result<(), String>) {
        if !config.general.keyring {
            return (config.clone(), Ok(()));
        }
        let state = self.state();
        self.set_state(State { on: true, ..state });
        match self.write(config, state.whole) {
            Ok(()) => (config.without_secrets(), Ok(())),
            Err(reason) => (config.clone(), Err(reason)),
        }
    }

    fn write(&self, config: &Config, whole: bool) -> Result<(), String> {
        let mut held = BTreeSet::new();
        for (name, value) in config.secrets() {
            if !value.is_empty() {
                keep(&*self.store, &name, &value)?;
                held.insert(name);
            }
        }
        if whole {
            for name in self.store.names()? {
                if !held.contains(&name) {
                    self.store.remove(&name)?;
                }
            }
        }
        Ok(())
    }

    /// After the file was written: turned off, the keyring gives up the keys
    /// the file now holds. When it never answered, the file does not hold
    /// them, and they stay there.
    pub fn after_save(&self, config: &Config) -> Result<(), String> {
        let state = self.state();
        if config.general.keyring || !state.on {
            return Ok(());
        }
        self.set_state(State { on: false, ..state });
        if !state.whole {
            return Ok(());
        }
        for name in self.store.names()? {
            self.store.remove(&name)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    struct Fake {
        keys: Mutex<BTreeMap<String, String>>,
        down: AtomicBool,
        forgetful: AtomicBool,
        hangs: AtomicBool,
        calls: AtomicUsize,
    }

    impl Fake {
        fn up(&self) -> Result<(), String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.hangs.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_secs(5));
            }
            if self.down.load(Ordering::SeqCst) {
                return Err("locked".into());
            }
            Ok(())
        }

        fn key(&self, name: &str) -> Option<String> {
            self.keys.lock().unwrap().get(name).cloned()
        }
    }

    impl SecretStore for Fake {
        fn names(&self) -> Result<Vec<String>, String> {
            self.up()?;
            Ok(self.keys.lock().unwrap().keys().cloned().collect())
        }

        fn get(&self, name: &str) -> Result<Option<String>, String> {
            self.up()?;
            Ok(self.key(name))
        }

        fn set(&self, name: &str, value: &str) -> Result<(), String> {
            self.up()?;
            if !self.forgetful.load(Ordering::SeqCst) {
                self.keys.lock().unwrap().insert(name.into(), value.into());
            }
            Ok(())
        }

        fn remove(&self, name: &str) -> Result<(), String> {
            self.up()?;
            self.keys.lock().unwrap().remove(name);
            Ok(())
        }
    }

    const JIMAKU: &str = "online.jimaku_api_key";

    fn config(keyring: bool, jimaku: &str) -> Config {
        let mut c = Config::default();
        c.general.keyring = keyring;
        c.online.jimaku_api_key = jimaku.into();
        c
    }

    fn keeper() -> (Arc<Fake>, Keeper) {
        let fake = Arc::new(Fake::default());
        let store: Arc<dyn SecretStore> = fake.clone();
        (fake, Keeper::new(store))
    }

    // T-MET-005: off, the keyring is never asked
    #[test]
    fn off_nothing_touches_the_keyring() {
        let (fake, keeper) = keeper();
        let mut c = config(false, "key");
        assert_eq!(keeper.open(&mut c), Ok(false));
        let (file, kept) = keeper.before_save(&c);
        keeper.after_save(&c).unwrap();
        assert_eq!(kept, Ok(()));
        assert_eq!(file.online.jimaku_api_key, "key");
        assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    }

    // T-MET-005
    #[test]
    fn turned_on_the_keys_leave_the_file() {
        let (fake, keeper) = keeper();
        let mut c = config(false, "key");
        keeper.open(&mut c).unwrap();
        c.general.keyring = true;
        let (file, kept) = keeper.before_save(&c);
        keeper.after_save(&c).unwrap();
        assert_eq!(kept, Ok(()));
        assert_eq!(file.online.jimaku_api_key, "");
        assert!(file.general.keyring);
        assert_eq!(fake.key(JIMAKU).as_deref(), Some("key"));
        assert_eq!(c.online.jimaku_api_key, "key", "memory keeps them");
    }

    // T-MET-005: the file loses a key only once the keyring gives it back
    #[test]
    fn a_keyring_that_fails_leaves_the_keys_in_the_file() {
        for forgetful in [false, true] {
            let (fake, keeper) = keeper();
            let c = config(true, "key");
            fake.down.store(!forgetful, Ordering::SeqCst);
            fake.forgetful.store(forgetful, Ordering::SeqCst);
            let (file, kept) = keeper.before_save(&c);
            assert!(kept.is_err());
            assert_eq!(file.online.jimaku_api_key, "key");
        }
    }

    // T-MET-005
    #[test]
    fn the_next_start_reads_them_back() {
        let (fake, keeper) = keeper();
        fake.keys
            .lock()
            .unwrap()
            .insert(JIMAKU.into(), "key".into());
        let mut c = config(true, "");
        assert_eq!(keeper.open(&mut c), Ok(false));
        assert_eq!(c.online.jimaku_api_key, "key");
    }

    // T-MET-005: a failed save left it in the file, the file wins
    #[test]
    fn a_key_in_the_file_moves_at_the_start() {
        let (fake, keeper) = keeper();
        fake.keys
            .lock()
            .unwrap()
            .insert(JIMAKU.into(), "old".into());
        let mut c = config(true, "new");
        assert_eq!(keeper.open(&mut c), Ok(true));
        assert_eq!(c.online.jimaku_api_key, "new");
        assert_eq!(fake.key(JIMAKU).as_deref(), Some("new"));
    }

    // T-MET-005: a key cleared in the settings, a server removed
    #[test]
    fn what_the_settings_dropped_leaves_the_keyring() {
        let (fake, keeper) = keeper();
        fake.keys
            .lock()
            .unwrap()
            .insert(JIMAKU.into(), "key".into());
        fake.keys
            .lock()
            .unwrap()
            .insert("servers.gone.token".into(), "token".into());
        let mut c = config(true, "");
        keeper.open(&mut c).unwrap();
        c.online.jimaku_api_key.clear();
        let (_, kept) = keeper.before_save(&c);
        assert_eq!(kept, Ok(()));
        assert!(fake.keys.lock().unwrap().is_empty());
    }

    // T-MET-005: a locked keyring at the start costs nothing, whatever the
    // settings do after it
    #[test]
    fn a_keyring_that_did_not_answer_loses_nothing() {
        let (fake, keeper) = keeper();
        fake.keys
            .lock()
            .unwrap()
            .insert(JIMAKU.into(), "key".into());
        fake.down.store(true, Ordering::SeqCst);
        let mut c = config(true, "");
        assert_eq!(keeper.open(&mut c), Err("locked".into()));
        assert_eq!(c.online.jimaku_api_key, "");
        fake.down.store(false, Ordering::SeqCst);
        // saved with the key blank in memory
        let (file, kept) = keeper.before_save(&c);
        keeper.after_save(&c).unwrap();
        assert_eq!(kept, Ok(()));
        assert_eq!(file.online.jimaku_api_key, "");
        assert_eq!(fake.key(JIMAKU).as_deref(), Some("key"));
        // off, and on again
        c.general.keyring = false;
        let _ = keeper.before_save(&c);
        keeper.after_save(&c).unwrap();
        assert_eq!(fake.key(JIMAKU).as_deref(), Some("key"));
        c.general.keyring = true;
        let _ = keeper.before_save(&c);
        keeper.after_save(&c).unwrap();
        assert_eq!(fake.key(JIMAKU).as_deref(), Some("key"));
    }

    // T-MET-005
    #[test]
    fn a_keyring_that_hangs_does_not_stop_the_start() {
        let fake = Arc::new(Fake::default());
        fake.hangs.store(true, Ordering::SeqCst);
        let store: Arc<dyn SecretStore> = fake.clone();
        let keeper = Keeper::with_answer(store, Duration::from_millis(20));
        let mut c = config(true, "");
        assert_eq!(keeper.open(&mut c), Err("no answer".into()));
    }

    // T-MET-005
    #[test]
    fn turned_off_the_keys_go_back_to_the_file() {
        let (fake, keeper) = keeper();
        fake.keys
            .lock()
            .unwrap()
            .insert(JIMAKU.into(), "key".into());
        let mut c = config(true, "");
        keeper.open(&mut c).unwrap();
        c.general.keyring = false;
        let (file, kept) = keeper.before_save(&c);
        assert_eq!(kept, Ok(()));
        assert_eq!(file.online.jimaku_api_key, "key");
        assert_eq!(fake.key(JIMAKU).as_deref(), Some("key"), "not yet");
        keeper.after_save(&c).unwrap();
        assert!(fake.keys.lock().unwrap().is_empty());
    }
}
