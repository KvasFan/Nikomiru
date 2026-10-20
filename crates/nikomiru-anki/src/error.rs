// SPDX-License-Identifier: GPL-3.0-only

//! What can go wrong talking to AnkiConnect (ANK-002, ANK-004). The API key
//! never appears in a message: only the URL does, and it holds no key.

/// An AnkiConnect failure, each kind with its own message (ANK-004).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnkiError {
    /// The URL in the settings is not an `http`/`https` address with a host.
    #[error("not an AnkiConnect address: {0}")]
    BadUrl(String),
    /// Nothing answered: Anki is probably not open.
    #[error("AnkiConnect is not responding on {url}")]
    Refused {
        /// The AnkiConnect URL.
        url: String,
    },
    /// Connected, but no reply in time: the port is probably another program's.
    #[error("no reply from {url} in time")]
    Timeout {
        /// The AnkiConnect URL.
        url: String,
    },
    /// The connection broke, or the server answered with an HTTP error.
    #[error("HTTP error from {url}: {reason}")]
    Http {
        /// The AnkiConnect URL.
        url: String,
        /// What happened.
        reason: String,
    },
    /// The reply is not AnkiConnect's: not JSON, or without `result` and
    /// `error`.
    #[error("unexpected reply from {url}: {reason}")]
    Protocol {
        /// The AnkiConnect URL.
        url: String,
        /// What was wrong with it.
        reason: String,
    },
    /// AnkiConnect refused the API key.
    #[error("AnkiConnect refused the API key")]
    WrongKey,
    /// The action is unknown to this AnkiConnect: it is too old.
    #[error("AnkiConnect does not know {0}")]
    Unsupported(String),
    /// Anki has no profile open.
    #[error("Anki has no profile open")]
    NoCollection,
    /// Anki is busy with a collection operation (AnkiConnect 26.10).
    #[error("Anki is busy")]
    Busy,
    /// An Anki editor window is open (AnkiConnect 26.10 refuses some writes
    /// then).
    #[error("an Anki editor window is open")]
    EditorOpen,
    /// The note type is not in Anki.
    #[error("note type not found: {0}")]
    ModelMissing(String),
    /// The deck is not in Anki.
    #[error("deck not found: {0}")]
    DeckMissing(String),
    /// `addNote`: the note is a duplicate.
    #[error("the card already exists")]
    Duplicate,
    /// `addNote`: the first field is empty.
    #[error("the first field is empty")]
    EmptyNote,
    /// The request was sent and the reply lost: the note may exist (ANK-003).
    #[error("{action}: the outcome is unknown, check in Anki")]
    CommitUnknown {
        /// The action.
        action: String,
    },
    /// Another error from AnkiConnect, in its words.
    #[error("AnkiConnect: {0}")]
    Anki(String),
}

impl AnkiError {
    /// The error AnkiConnect reported with `message` for `action`.
    pub fn from_anki(action: &str, message: &str) -> Self {
        let lower = message.to_lowercase();
        if lower.contains("valid api key must be provided") {
            AnkiError::WrongKey
        } else if lower == "unsupported action" {
            AnkiError::Unsupported(action.to_string())
        } else if lower.contains("collection is not available") {
            AnkiError::NoCollection
        } else if lower.contains("collection lifecycle operation is active")
            || lower.contains("a collection operation is active")
        {
            AnkiError::Busy
        } else if lower.starts_with("close the affected editor") {
            AnkiError::EditorOpen
        } else if action == "addNote" && message == crate::client::DUPLICATE {
            AnkiError::Duplicate
        } else if lower.starts_with("cannot create note because it is empty") {
            AnkiError::EmptyNote
        } else if let Some(name) = message.strip_prefix("model was not found: ") {
            AnkiError::ModelMissing(name.to_string())
        } else if let Some(name) = message.strip_prefix("deck was not found: ") {
            AnkiError::DeckMissing(name.to_string())
        } else {
            AnkiError::Anki(message.to_string())
        }
    }

    /// A failure of the connection itself, which starts the 30 s pause of the
    /// duplicate checks (ANK-030).
    pub fn is_transport(&self) -> bool {
        matches!(
            self,
            AnkiError::Refused { .. } | AnkiError::Timeout { .. } | AnkiError::Http { .. }
        )
    }
}

impl nikomiru_core::Localized for AnkiError {
    fn message_id(&self) -> &'static str {
        match self {
            AnkiError::BadUrl(_) => "error-anki-bad-url",
            AnkiError::Refused { .. } => "error-anki-refused",
            AnkiError::Timeout { .. } => "error-anki-timeout",
            AnkiError::Http { .. } => "error-anki-http",
            AnkiError::Protocol { .. } => "error-anki-protocol",
            AnkiError::WrongKey => "error-anki-wrong-key",
            AnkiError::Unsupported(_) => "error-anki-unsupported",
            AnkiError::NoCollection => "error-anki-no-collection",
            AnkiError::Busy => "error-anki-busy",
            AnkiError::EditorOpen => "error-anki-editor-open",
            AnkiError::ModelMissing(_) => "error-anki-model-missing",
            AnkiError::DeckMissing(_) => "error-anki-deck-missing",
            AnkiError::Duplicate => "error-anki-duplicate",
            AnkiError::EmptyNote => "error-anki-empty-note",
            AnkiError::CommitUnknown { .. } => "error-anki-commit-unknown",
            AnkiError::Anki(_) => "error-anki",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            AnkiError::BadUrl(url) | AnkiError::Refused { url } | AnkiError::Timeout { url } => {
                vec![("url", url.clone())]
            }
            AnkiError::Http { url, reason } | AnkiError::Protocol { url, reason } => {
                vec![("url", url.clone()), ("reason", reason.clone())]
            }
            AnkiError::Unsupported(action) | AnkiError::CommitUnknown { action } => {
                vec![("action", action.clone())]
            }
            AnkiError::ModelMissing(name) | AnkiError::DeckMissing(name) => {
                vec![("name", name.clone())]
            }
            AnkiError::Anki(reason) => vec![("reason", reason.clone())],
            AnkiError::WrongKey
            | AnkiError::NoCollection
            | AnkiError::Busy
            | AnkiError::EditorOpen
            | AnkiError::Duplicate
            | AnkiError::EmptyNote => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-ANK-002
    #[test]
    fn anki_messages_become_typed_errors() {
        let cases = [
            (
                "deckNames",
                "valid api key must be provided",
                AnkiError::WrongKey,
            ),
            (
                "canAddNotesWithErrorDetail",
                "unsupported action",
                AnkiError::Unsupported("canAddNotesWithErrorDetail".into()),
            ),
            (
                "deckNames",
                "collection is not available",
                AnkiError::NoCollection,
            ),
            (
                "guiBrowse",
                "A collection operation is active.",
                AnkiError::Busy,
            ),
            (
                "createModel",
                "Close the affected editor before this API write, even if its content is saved.",
                AnkiError::EditorOpen,
            ),
            (
                "addNote",
                "cannot create note because it is a duplicate",
                AnkiError::Duplicate,
            ),
            (
                "addNote",
                "cannot create note because it is empty",
                AnkiError::EmptyNote,
            ),
            (
                "modelFieldNames",
                "model was not found: Lapis",
                AnkiError::ModelMissing("Lapis".into()),
            ),
            (
                "addNote",
                "deck was not found: Mining",
                AnkiError::DeckMissing("Mining".into()),
            ),
            (
                "findNotes",
                "something else",
                AnkiError::Anki("something else".into()),
            ),
        ];
        for (action, message, expected) in cases {
            assert_eq!(AnkiError::from_anki(action, message), expected, "{message}");
        }
        // only addNote has a duplicate error
        assert_eq!(
            AnkiError::from_anki(
                "updateNoteFields",
                "cannot create note because it is a duplicate"
            ),
            AnkiError::Anki("cannot create note because it is a duplicate".into())
        );
    }

    // T-ERR-001, T-ANK-004
    #[test]
    fn every_error_has_its_messages() {
        use nikomiru_core::Localized;
        let url = "http://127.0.0.1:8765".to_string();
        let errors = [
            AnkiError::BadUrl("x".into()),
            AnkiError::Refused { url: url.clone() },
            AnkiError::Timeout { url: url.clone() },
            AnkiError::Http {
                url: url.clone(),
                reason: "x".into(),
            },
            AnkiError::Protocol {
                url,
                reason: "x".into(),
            },
            AnkiError::WrongKey,
            AnkiError::Unsupported("x".into()),
            AnkiError::NoCollection,
            AnkiError::Busy,
            AnkiError::EditorOpen,
            AnkiError::ModelMissing("x".into()),
            AnkiError::DeckMissing("x".into()),
            AnkiError::Duplicate,
            AnkiError::EmptyNote,
            AnkiError::CommitUnknown {
                action: "addNote".into(),
            },
            AnkiError::Anki("x".into()),
        ];
        let all: Vec<&dyn Localized> = errors.iter().map(|e| e as _).collect();
        let dir = std::path::Path::new(nikomiru_core::error::I18N_DIR);
        let problems = nikomiru_core::error::message_problems(dir, &all);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
