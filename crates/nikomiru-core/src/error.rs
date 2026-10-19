// SPDX-License-Identifier: GPL-3.0-only

//! Errors the interface shows translated (ERR-001). Every public error type
//! implements [`Localized`] with an exhaustive `match`, so a new variant does
//! not compile until it has a message; the T-ERR-001 test checks that every
//! message exists in every language. `Display` stays English, for the log.

/// An error with a Fluent message.
pub trait Localized {
    /// Fluent message id.
    fn message_id(&self) -> &'static str;

    /// The message's arguments (`$name` → value).
    fn message_args(&self) -> Vec<(&'static str, String)>;
}

/// The problems with the messages of `errors` in the Fluent files under `i18n`
/// (`<language>/main.ftl`): a message missing in a language, or a `{ $variable }`
/// the message uses but the error does not give. For the T-ERR-001 tests of the
/// crates that define errors.
#[doc(hidden)]
pub fn message_problems(i18n: &std::path::Path, errors: &[&dyn Localized]) -> Vec<String> {
    let mut problems = Vec::new();
    let Ok(dirs) = std::fs::read_dir(i18n) else {
        return vec![format!("{} cannot be read", i18n.display())];
    };
    let mut languages = 0;
    for dir in dirs.flatten() {
        let file = dir.path().join("main.ftl");
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        languages += 1;
        let lang = dir.file_name().to_string_lossy().into_owned();
        for e in errors {
            let id = e.message_id();
            let Some(value) = source.lines().find_map(|l| {
                l.strip_prefix(id)
                    .and_then(|rest| rest.trim_start().strip_prefix('='))
            }) else {
                problems.push(format!("{lang}: no message {id}"));
                continue;
            };
            let given: Vec<&str> = e.message_args().iter().map(|(k, _)| *k).collect();
            for var in value.split('$').skip(1) {
                let name: String = var
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                if !given.contains(&name.as_str()) {
                    problems.push(format!("{lang}: {id} uses ${name}, not given"));
                }
            }
        }
    }
    if languages == 0 {
        problems.push(format!("no languages in {}", i18n.display()));
    }
    problems
}

/// The interface strings, for [`message_problems`] in tests.
#[doc(hidden)]
pub const I18N_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/i18n");

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(&'static str, Vec<(&'static str, String)>);

    impl Localized for Fake {
        fn message_id(&self) -> &'static str {
            self.0
        }
        fn message_args(&self) -> Vec<(&'static str, String)> {
            self.1.clone()
        }
    }

    #[test]
    fn missing_messages_and_arguments_are_found() {
        let dir = std::path::Path::new(I18N_DIR);
        let ok = Fake(
            "error-file",
            vec![("file", "a".into()), ("reason", "b".into())],
        );
        assert!(message_problems(dir, &[&ok]).is_empty());
        let no_reason = Fake("error-file", vec![("file", "a".into())]);
        let problems = message_problems(dir, &[&no_reason]);
        assert!(
            problems.iter().any(|p| p.contains("$reason")),
            "{problems:?}"
        );
        let missing = Fake("error-does-not-exist", Vec::new());
        assert_eq!(message_problems(dir, &[&missing]).len(), 2, "en and it");
    }
}
