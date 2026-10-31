// SPDX-License-Identifier: GPL-3.0-only

//! Where the videos of media servers are (SRV-007), in a table of its own
//! (§8 `server_items`): such a video is a `videos` record like any other, its
//! uid and path being `srv:<server id>:<item id>`, so progress, statistics and
//! cards work as they do for files, and a library without the table is read
//! as one with no server videos.

use redb::TableDefinition;
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError, db_err};

/// By uid.
const SERVER_ITEMS: TableDefinition<&str, &[u8]> = TableDefinition::new("server_items");

/// What starts the uid of a server's video.
const PREFIX: &str = "srv:";

/// The uid, and path, of an item of a server.
pub fn server_uid(server: &str, item: &str) -> String {
    format!("{PREFIX}{server}:{item}")
}

/// The server and item of a uid, if it is one of a server's video.
pub fn parse_server_uid(uid: &str) -> Option<(&str, &str)> {
    uid.strip_prefix(PREFIX)?.split_once(':')
}

/// A video of a media server.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ServerItemRecord {
    /// The title of its series, empty for a movie.
    pub series: String,
    /// Its own title.
    pub title: String,
    /// Its AniList id, when the server has one (SRV-010).
    pub anilist_id: Option<u64>,
}

impl Library {
    /// What is known of the server's video `uid`.
    pub fn server_item(&self, uid: &str) -> Result<Option<ServerItemRecord>, LibraryError> {
        self.read(|tx| match tx.open_table(SERVER_ITEMS) {
            Ok(t) => match db_err!(t.get(uid))? {
                Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
                None => Ok(None),
            },
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// Saves what is known of the server's video `uid`.
    pub fn set_server_item(
        &self,
        uid: &str,
        record: &ServerItemRecord,
    ) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(SERVER_ITEMS))?;
            let bytes = postcard::to_allocvec(record)?;
            db_err!(t.insert(uid, bytes.as_slice()))?;
        }
        db_err!(tx.commit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SRV-007
    #[test]
    fn a_server_video_has_its_uid() {
        let uid = server_uid("home", "item:7");
        assert_eq!(uid, "srv:home:item:7");
        assert_eq!(parse_server_uid(&uid), Some(("home", "item:7")));
        assert_eq!(parse_server_uid("video/Episode 1"), None);
        assert_eq!(parse_server_uid("srv:broken"), None);
    }

    // T-SRV-007: a library from before the table reads as having none
    #[test]
    fn the_table_is_optional() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        assert_eq!(lib.server_item("srv:a:b").unwrap(), None);
        let record = ServerItemRecord {
            series: "A Series".into(),
            title: "Episode 1".into(),
            anilist_id: Some(5),
        };
        lib.set_server_item("srv:a:b", &record).unwrap();
        assert_eq!(lib.server_item("srv:a:b").unwrap(), Some(record));
        assert_eq!(lib.server_item("srv:a:c").unwrap(), None);
    }
}
