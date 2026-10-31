// SPDX-License-Identifier: GPL-3.0-only

//! WebDAV for the sync (BAK-004, BAK-006): the folder `nikomiru/devices` under
//! the address the user gave, and one file per device. What a server lists is
//! untrusted: only names of device files are believed.

use reqwest::Url;

use crate::error::OnlineError;
use crate::http::{Http, Response};
use crate::servers::{checked_server_url, http_for, join};

const NAME: &str = "WebDAV";
/// The extension of a device's file.
const EXTENSION: &str = ".sync";

/// A WebDAV folder.
pub struct WebDav {
    http: Http,
    base: Url,
    user: String,
    password: String,
}

impl std::fmt::Debug for WebDav {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebDav")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

/// A device id Nikomiru itself would have made: nothing else is a file name.
fn is_device(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The devices named in a `PROPFIND` answer: the end of each `href`.
fn devices_in(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in xml.split('<').filter(|p| {
        p.split('>')
            .next()
            .is_some_and(|tag| tag.to_ascii_lowercase().ends_with("href"))
    }) {
        let Some((_, href)) = part.split_once('>') else {
            continue;
        };
        let name = href.trim().rsplit('/').next().unwrap_or_default();
        if let Some(device) = name.strip_suffix(EXTENSION)
            && is_device(device)
            && !out.iter().any(|d| d == device)
        {
            out.push(device.to_string());
        }
    }
    out.sort();
    out
}

fn checked(response: Response) -> Result<Response, OnlineError> {
    if response.status == 507 {
        return Err(OnlineError::Quota {
            provider: NAME.into(),
        });
    }
    response.ok(NAME)
}

impl WebDav {
    /// The folder at `url`, under the rules of SRV-005.
    pub fn new(
        url: &str,
        user: &str,
        password: &str,
        allow_unencrypted: bool,
        proxy: &str,
    ) -> Result<Self, OnlineError> {
        let mut base = checked_server_url(url, allow_unencrypted)?;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        Ok(Self {
            http: http_for(&base, proxy)?,
            base,
            user: user.to_string(),
            password: password.to_string(),
        })
    }

    fn dav(
        &self,
        method: &str,
        url: &Url,
        depth: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<Response, OnlineError> {
        self.http
            .dav(method, url, (&self.user, &self.password), depth, body)
    }

    fn folder(&self, levels: &[&str]) -> Result<Url, OnlineError> {
        let mut url = join(&self.base, levels)?;
        let path = format!("{}/", url.path());
        url.set_path(&path);
        Ok(url)
    }

    /// Makes the folders, if they are not there: a check that they can be
    /// written, too.
    pub fn prepare(&self) -> Result<(), OnlineError> {
        for levels in [&["nikomiru"][..], &["nikomiru", "devices"][..]] {
            let response = self.dav("MKCOL", &self.folder(levels)?, None, None)?;
            // 405: it is there
            if response.status != 405 {
                checked(response)?;
            }
        }
        Ok(())
    }

    /// The devices with a file.
    pub fn devices(&self) -> Result<Vec<String>, OnlineError> {
        let url = self.folder(&["nikomiru", "devices"])?;
        let response = checked(self.dav("PROPFIND", &url, Some("1"), None)?)?;
        Ok(devices_in(&String::from_utf8_lossy(&response.body)))
    }

    fn file(&self, device: &str) -> Result<Url, OnlineError> {
        if !is_device(device) {
            return Err(OnlineError::BadUrl(device.to_string()));
        }
        let name = format!("{device}{EXTENSION}");
        join(&self.base, &["nikomiru", "devices", &name])
    }

    /// A device's file.
    pub fn read(&self, device: &str) -> Result<Vec<u8>, OnlineError> {
        Ok(checked(self.dav("GET", &self.file(device)?, None, None)?)?.body)
    }

    /// Writes this device's file.
    pub fn write(&self, device: &str, bytes: Vec<u8>) -> Result<(), OnlineError> {
        checked(self.dav("PUT", &self.file(device)?, None, Some(bytes))?).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-BAK-010: only device names are believed
    #[test]
    fn a_listing_gives_only_devices() {
        let xml = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:">
            <d:response><d:href>/dav/nikomiru/devices/</d:href></d:response>
            <d:response><d:href>/dav/nikomiru/devices/abc-1.sync</d:href></d:response>
            <D:response><D:HREF>/dav/nikomiru/devices/def.sync</D:HREF></D:response>
            <d:response><d:href>/dav/nikomiru/devices/../../evil.sync</d:href></d:response>
            <d:response><d:href>/dav/nikomiru/devices/a%2Fb.sync</d:href></d:response>
            <d:response><d:href>/dav/nikomiru/devices/notes.txt</d:href></d:response>
            </d:multistatus>"#;
        assert_eq!(devices_in(xml), ["abc-1", "def", "evil"]);
        assert!(devices_in("").is_empty());
        assert!(devices_in("<<<>>>href>").is_empty());
    }

    // T-BAK-004
    #[test]
    fn addresses_follow_the_rules_of_servers() {
        assert!(WebDav::new("http://nas/dav", "u", "p", false, "").is_ok());
        assert!(WebDav::new("http://cloud.example.org/dav", "u", "p", false, "").is_err());
        let dav = WebDav::new("https://cloud.example.org/dav", "u", "s3cret", false, "").unwrap();
        assert!(!format!("{dav:?}").contains("s3cret"));
        assert_eq!(
            dav.file("abc").unwrap().as_str(),
            "https://cloud.example.org/dav/nikomiru/devices/abc.sync"
        );
        assert!(dav.file("../x").is_err());
    }
}
