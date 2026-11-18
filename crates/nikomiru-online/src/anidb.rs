// SPDX-License-Identifier: GPL-3.0-only

//! AniDB file identification (MET-006): a video's ed2k hash and size, asked
//! of AniDB's UDP API with the user's account, give the series and episode
//! the file belongs to.
//!
//! AniDB allows only registered clients: [`CLIENT`] holds the maintainer's
//! registration once it exists; until then a user can put their own in the
//! settings. Packets are spaced as the API asks (one every
//! [`INTERVAL`]), and nothing is sent before the user asks.

use std::io::Read;
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use md4::{Digest, Md4};

use crate::error::OnlineError;

/// The provider's name in errors.
const PROVIDER: &str = "AniDB";

/// AniDB's UDP API.
pub const ADDRESS: &str = "api.anidb.net:9000";

/// The client registered for Nikomiru at AniDB (name, version), once the
/// maintainer has registered it; `None` until then.
pub const CLIENT: Option<(&str, u32)> = None;

/// The time between two packets: AniDB asks for no more than one every
/// four seconds over a long time.
pub const INTERVAL: Duration = Duration::from_secs(4);

/// How long an answer is waited for.
const TIMEOUT: Duration = Duration::from_secs(15);

/// The size of an ed2k chunk.
pub const CHUNK: u64 = 9_728_000;

/// The largest answer read.
const MAX_ANSWER: usize = 1400;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The ed2k hashes of `size` bytes read from `reader`: the usual one, and,
/// when the size is a whole number of chunks, the other variant (an empty
/// chunk's hash appended), which some files are known by.
pub fn ed2k(
    reader: &mut dyn Read,
    size: u64,
    stop: &dyn Fn() -> bool,
) -> std::io::Result<(String, Option<String>)> {
    let mut chunks: Vec<[u8; 16]> = Vec::new();
    let mut buffer = vec![0u8; 1 << 20];
    let mut left = size;
    while left > 0 {
        let mut hasher = Md4::new();
        let mut in_chunk = CHUNK.min(left);
        left -= in_chunk;
        while in_chunk > 0 {
            if stop() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "stopped",
                ));
            }
            let want = (buffer.len() as u64).min(in_chunk) as usize;
            let read = reader.read(&mut buffer[..want])?;
            if read == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            hasher.update(&buffer[..read]);
            in_chunk -= read as u64;
        }
        chunks.push(hasher.finalize().into());
    }
    let empty: [u8; 16] = Md4::digest(b"").into();
    let of_list = |list: &[[u8; 16]]| {
        let mut hasher = Md4::new();
        for h in list {
            hasher.update(h);
        }
        hex(&hasher.finalize())
    };
    let usual = match chunks.as_slice() {
        [] => hex(&empty),
        [one] => hex(one),
        many => of_list(many),
    };
    let other = (size > 0 && size.is_multiple_of(CHUNK)).then(|| {
        let mut list = chunks.clone();
        list.push(empty);
        of_list(&list)
    });
    Ok((usual, other))
}

/// A value in a request: `&` and line ends written as AniDB reads them.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace("\r\n", "<br />")
        .replace('\n', "<br />")
}

/// A value in an answer: AniDB writes `'` as `` ` `` and line ends as `<br />`.
fn unescape(value: &str) -> String {
    value.replace('`', "'").replace("<br />", "\n")
}

/// What AniDB knows of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// The anime's id.
    pub aid: u64,
    /// The episode's id.
    pub eid: u64,
    /// The anime's episodes, 0 when not known.
    pub episodes: u32,
    /// Its year, as written (`2019`, `2019-2020`).
    pub year: String,
    /// Its romaji title.
    pub romaji: String,
    /// Its Japanese title.
    pub kanji: String,
    /// Its English title.
    pub english: String,
    /// The episode's number, as written (`1`, `S1`, `C2`).
    pub epno: String,
    /// The episode's English title.
    pub episode_title: String,
}

impl FileInfo {
    /// Its titles, the romaji first, without empty ones.
    pub fn titles(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for t in [&self.romaji, &self.english, &self.kanji] {
            if !t.is_empty() && !out.contains(t) {
                out.push(t.clone());
            }
        }
        out
    }
}

/// `fmask`: the anime's and the episode's ids.
pub const FMASK: &str = "6000000000";
/// `amask`: episodes, year, romaji, Japanese and English titles, the
/// episode's number and English title.
pub const AMASK: &str = "A0E0C000";

/// The answer to a `FILE` request: its fields after the file id, in the
/// order of [`FMASK`] then [`AMASK`].
fn parse_file(data: &str) -> Result<FileInfo, OnlineError> {
    let fields: Vec<&str> = data.trim_end_matches(['\n', '\r']).split('|').collect();
    let bad = || OnlineError::Protocol {
        provider: PROVIDER.into(),
        reason: "a file answer with other fields".into(),
    };
    let [
        _fid,
        aid,
        eid,
        episodes,
        year,
        romaji,
        kanji,
        english,
        epno,
        title,
    ] = fields.as_slice()
    else {
        return Err(bad());
    };
    Ok(FileInfo {
        aid: aid.parse().map_err(|_| bad())?,
        eid: eid.parse().map_err(|_| bad())?,
        episodes: episodes.parse().unwrap_or(0),
        year: unescape(year),
        romaji: unescape(romaji),
        kanji: unescape(kanji),
        english: unescape(english),
        epno: unescape(epno),
        episode_title: unescape(title),
    })
}

/// An answer: its code, its text, and its data line.
fn split_answer(answer: &str) -> Result<(u16, &str, &str), OnlineError> {
    let (first, data) = answer.split_once('\n').unwrap_or((answer, ""));
    let (code, text) = first.split_once(' ').unwrap_or((first, ""));
    let code = code.parse().map_err(|_| OnlineError::Protocol {
        provider: PROVIDER.into(),
        reason: "an answer without a code".into(),
    })?;
    Ok((code, text.trim(), data))
}

/// The error of an answer code that is not the one expected.
fn error_of(code: u16, text: &str) -> OnlineError {
    match code {
        500 | 502 | 505 => OnlineError::Unauthorized {
            provider: PROVIDER.into(),
        },
        503 => OnlineError::Protocol {
            provider: PROVIDER.into(),
            reason: "this client version is no longer accepted".into(),
        },
        504 | 555 => OnlineError::Protocol {
            provider: PROVIDER.into(),
            reason: format!("banned: {text}"),
        },
        601 | 602 | 604 => OnlineError::RateLimited {
            provider: PROVIDER.into(),
            seconds: 1800,
        },
        _ => OnlineError::Protocol {
            provider: PROVIDER.into(),
            reason: format!("{code} {text}"),
        },
    }
}

/// An account at AniDB and the client to use it with.
#[derive(Clone)]
pub struct Account {
    /// The user name.
    pub user: String,
    /// The password.
    pub password: String,
    /// The registered client's name.
    pub client: String,
    /// Its version.
    pub version: u32,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // never the password (PRV-002)
        f.debug_struct("Account")
            .field("user", &self.user)
            .field("client", &self.client)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// A connection to AniDB's UDP API: logged in on the first request, packets
/// spaced by an interval, logged out when dropped.
pub struct AniDb {
    socket: UdpSocket,
    account: Account,
    session: Option<String>,
    interval: Duration,
    last: Option<Instant>,
    sleep: Box<dyn Fn(Duration) + Send>,
}

impl std::fmt::Debug for AniDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AniDb")
            .field("account", &self.account)
            .finish_non_exhaustive()
    }
}

impl AniDb {
    /// A client of the API at `address`, packets `interval` apart, waiting
    /// with `sleep`.
    pub fn new(
        address: &str,
        account: Account,
        interval: Duration,
        sleep: Box<dyn Fn(Duration) + Send>,
    ) -> Result<Self, OnlineError> {
        if account.client.is_empty() || account.version == 0 {
            return Err(OnlineError::NoKey {
                provider: "AniDB client".into(),
            });
        }
        if account.user.is_empty() || account.password.is_empty() {
            return Err(OnlineError::NoKey {
                provider: PROVIDER.into(),
            });
        }
        let unreachable = |reason: String| OnlineError::Unreachable {
            host: address.to_string(),
            reason,
        };
        let target = address
            .to_socket_addrs()
            .map_err(|e| unreachable(e.to_string()))?
            .next()
            .ok_or_else(|| unreachable("no address".into()))?;
        let local = if target.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let socket = UdpSocket::bind(local).map_err(|e| unreachable(e.to_string()))?;
        socket
            .connect(target)
            .map_err(|e| unreachable(e.to_string()))?;
        socket
            .set_read_timeout(Some(TIMEOUT))
            .map_err(|e| unreachable(e.to_string()))?;
        Ok(Self {
            socket,
            account,
            session: None,
            interval,
            last: None,
            sleep,
        })
    }

    /// Sends `request` in its turn and reads the answer.
    fn ask(&mut self, request: &str) -> Result<String, OnlineError> {
        if let Some(last) = self.last {
            let since = last.elapsed();
            if since < self.interval {
                (self.sleep)(self.interval - since);
            }
        }
        self.last = Some(Instant::now());
        let host = || ADDRESS.to_string();
        self.socket
            .send(request.as_bytes())
            .map_err(|e| OnlineError::Unreachable {
                host: host(),
                reason: e.to_string(),
            })?;
        let mut buffer = [0u8; MAX_ANSWER];
        let read = self.socket.recv(&mut buffer).map_err(|e| match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                OnlineError::Timeout { host: host() }
            }
            _ => OnlineError::Unreachable {
                host: host(),
                reason: e.to_string(),
            },
        })?;
        String::from_utf8(buffer[..read].to_vec()).map_err(|_| OnlineError::Protocol {
            provider: PROVIDER.into(),
            reason: "an answer that is not UTF-8".into(),
        })
    }

    /// Logs in, once.
    fn login(&mut self) -> Result<String, OnlineError> {
        if let Some(session) = &self.session {
            return Ok(session.clone());
        }
        let a = &self.account;
        let request = format!(
            "AUTH user={}&pass={}&protover=3&client={}&clientver={}&enc=UTF8",
            escape(&a.user),
            escape(&a.password),
            escape(&a.client),
            a.version
        );
        let answer = self.ask(&request)?;
        let (code, text, _) = split_answer(&answer)?;
        match code {
            200 | 201 => {
                let session = text
                    .split_whitespace()
                    .next()
                    .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric()) && !s.is_empty())
                    .ok_or_else(|| OnlineError::Protocol {
                        provider: PROVIDER.into(),
                        reason: "a login without a session".into(),
                    })?
                    .to_string();
                self.session = Some(session.clone());
                Ok(session)
            }
            _ => Err(error_of(code, text)),
        }
    }

    /// The file of `size` bytes and ed2k `hash`, or `None` when AniDB does
    /// not know it.
    pub fn file(&mut self, size: u64, hash: &str) -> Result<Option<FileInfo>, OnlineError> {
        let session = self.login()?;
        let request =
            format!("FILE size={size}&ed2k={hash}&fmask={FMASK}&amask={AMASK}&s={session}");
        let answer = self.ask(&request)?;
        let (code, text, data) = split_answer(&answer)?;
        match code {
            220 => parse_file(data).map(Some),
            320 => Ok(None),
            501 | 506 => {
                // the session ended: once more with a new one
                self.session = None;
                let session = self.login()?;
                let request =
                    format!("FILE size={size}&ed2k={hash}&fmask={FMASK}&amask={AMASK}&s={session}");
                let answer = self.ask(&request)?;
                let (code, text, data) = split_answer(&answer)?;
                match code {
                    220 => parse_file(data).map(Some),
                    320 => Ok(None),
                    _ => Err(error_of(code, text)),
                }
            }
            _ => Err(error_of(code, text)),
        }
    }
}

impl Drop for AniDb {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = self.ask(&format!("LOGOUT s={session}"));
        }
    }
}

/// Identifies the file at `path` (MET-006): its hash, then AniDB; the other
/// ed2k variant is asked only when the usual one is unknown and the size is a
/// whole number of chunks.
pub fn identify(
    anidb: &mut AniDb,
    path: &std::path::Path,
    stop: &dyn Fn() -> bool,
) -> Result<Option<FileInfo>, OnlineError> {
    let file_error = |e: std::io::Error| OnlineError::Protocol {
        provider: PROVIDER.into(),
        reason: format!("{}: {e}", path.display()),
    };
    let mut file = std::fs::File::open(path).map_err(file_error)?;
    let size = file.metadata().map_err(file_error)?.len();
    let (usual, other) = ed2k(&mut file, size, stop).map_err(file_error)?;
    if let Some(found) = anidb.file(size, &usual)? {
        return Ok(Some(found));
    }
    match other {
        Some(other) => anidb.file(size, &other),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(data: &[u8]) -> (String, Option<String>) {
        ed2k(&mut &data[..], data.len() as u64, &|| false).unwrap()
    }

    // T-MET-006: the ed2k hash, with AniDB's published vectors.
    #[test]
    fn ed2k_hashes() {
        assert_eq!(hash(b"").0, "31d6cfe0d16ae931b73c59d7e0c089c0");
        assert_eq!(hash(b"abc").0, "a448017aaf21d8525fc10ae87aa6729d");
        assert_eq!(hash(b"abc").1, None);
        let zeros = vec![0u8; CHUNK as usize];
        let (usual, other) = hash(&zeros);
        assert_eq!(usual, "d7def262a127cd79096a108e7a9fc138");
        assert_eq!(other.as_deref(), Some("fc21d9af828f92a8df64beac3357425d"));
        let more = vec![0u8; CHUNK as usize + 1];
        assert_eq!(hash(&more).1, None);
        assert_ne!(hash(&more).0, usual);
        // a short file is an error, a stop interrupts
        assert!(ed2k(&mut &b"ab"[..], 3, &|| false).is_err());
        assert!(ed2k(&mut &zeros[..], CHUNK, &|| true).is_err());
    }

    // T-MET-006: answers and values as AniDB writes them.
    #[test]
    fn answers_are_read() {
        let (code, text, data) = split_answer(
            "220 FILE\n7|11|22|12|2019|Kimetsu no Yaiba|鬼滅の刃|Demon Slayer|1|Cruelty\n",
        )
        .unwrap();
        assert_eq!((code, text), (220, "FILE"));
        let info = parse_file(data).unwrap();
        assert_eq!((info.aid, info.eid, info.episodes), (11, 22, 12));
        assert_eq!(
            info.titles(),
            ["Kimetsu no Yaiba", "Demon Slayer", "鬼滅の刃"]
        );
        assert_eq!(info.epno, "1");
        assert!(parse_file("7|11|22").is_err());
        assert_eq!(unescape("It`s<br />new"), "It's\nnew");
        assert_eq!(escape("a&b\nc"), "a&amp;b<br />c");
        assert!(split_answer("FILE").is_err());
        assert!(matches!(
            error_of(500, "LOGIN FAILED"),
            OnlineError::Unauthorized { .. }
        ));
        assert!(matches!(
            error_of(601, "ANIDB OUT OF SERVICE"),
            OnlineError::RateLimited { .. }
        ));
    }

    #[test]
    fn accounts_never_show_their_password() {
        let a = Account {
            user: "me".into(),
            password: "s3cret".into(),
            client: "nikomiru".into(),
            version: 1,
        };
        assert!(!format!("{a:?}").contains("s3cret"));
    }
}
