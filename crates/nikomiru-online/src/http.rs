// SPDX-License-Identifier: GPL-3.0-only

//! HTTP for the online services (ADR 0008 §6): reqwest's blocking client with
//! native-tls, the proxy of `[network] proxy` and never the environment's,
//! timeouts, bodies read with a limit, HTTPS only outside loopback.

use std::io::Read;
use std::net::IpAddr;
use std::time::Duration;

use reqwest::Url;
use serde_json::Value;

use crate::error::OnlineError;

/// Time to connect.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Time for a whole request.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The largest answer of an API.
pub const ANSWER_LIMIT: usize = 4 << 20;

/// The largest downloaded file (MET-004).
pub const FILE_LIMIT: usize = 64 << 20;

/// An answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The status.
    pub status: u16,
    /// The body.
    pub body: Vec<u8>,
    /// `Retry-After`, in seconds.
    pub retry_after: Option<u64>,
    /// `Content-Type`, in lower case.
    pub content_type: Option<String>,
}

impl Response {
    /// The body as JSON.
    pub fn json(&self, provider: &str) -> Result<Value, OnlineError> {
        serde_json::from_slice(&self.body).map_err(|_| OnlineError::Protocol {
            provider: provider.to_string(),
            reason: "not JSON".into(),
        })
    }

    /// The answer itself when the status is 2xx, else the error it means.
    pub fn ok(self, provider: &str) -> Result<Self, OnlineError> {
        let provider = provider.to_string();
        match self.status {
            200..=299 => Ok(self),
            401 | 403 => Err(OnlineError::Unauthorized { provider }),
            406 => Err(OnlineError::Quota { provider }),
            429 => Err(OnlineError::RateLimited {
                provider,
                seconds: self.retry_after.unwrap_or(60),
            }),
            status => Err(OnlineError::Http { provider, status }),
        }
    }
}

/// Whether `url` names this computer.
pub fn is_loopback(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// `url` if Nikomiru may send a key there: HTTPS, or HTTP on loopback (the
/// tests' fake server).
pub fn checked_url(url: &str) -> Result<Url, OnlineError> {
    let bad = || OnlineError::BadUrl(nikomiru_core::config::without_userinfo(url));
    let parsed = Url::parse(url.trim()).map_err(|_| bad())?;
    match parsed.scheme() {
        "https" if parsed.host_str().is_some() => Ok(parsed),
        "http" if is_loopback(&parsed) => Ok(parsed),
        _ => Err(bad()),
    }
}

/// The HTTP clients of the online services.
#[derive(Debug, Clone)]
pub struct Http {
    /// For the APIs: no redirects, which would take the provider's headers
    /// elsewhere.
    api: reqwest::blocking::Client,
    /// For files: redirects allowed (reqwest drops `Authorization` when the
    /// host changes).
    files: reqwest::blocking::Client,
}

fn host_of(url: &Url) -> String {
    url.host_str().unwrap_or_default().to_string()
}

fn failure(url: &Url, e: &reqwest::Error) -> OnlineError {
    let host = host_of(url);
    if e.is_timeout() {
        OnlineError::Timeout { host }
    } else {
        let mut reason = e.to_string();
        let mut source: Option<&dyn std::error::Error> = std::error::Error::source(e);
        while let Some(s) = source {
            reason = s.to_string();
            source = s.source();
        }
        OnlineError::Unreachable { host, reason }
    }
}

impl Http {
    /// The clients, through `proxy` if it is not empty (SET-007).
    pub fn new(proxy: &str) -> Result<Self, OnlineError> {
        Self::with_timeouts(proxy, CONNECT_TIMEOUT, REQUEST_TIMEOUT)
    }

    /// Like [`Http::new`], with other timeouts (AUD-004).
    pub fn with_timeouts(
        proxy: &str,
        connect: Duration,
        request: Duration,
    ) -> Result<Self, OnlineError> {
        let build = |redirects: reqwest::redirect::Policy| {
            let mut builder = reqwest::blocking::Client::builder()
                .user_agent(concat!("Nikomiru/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(connect)
                .timeout(request)
                .redirect(redirects);
            builder = if proxy.trim().is_empty() {
                builder.no_proxy()
            } else {
                let p = reqwest::Proxy::all(proxy.trim()).map_err(|_| {
                    OnlineError::BadUrl(nikomiru_core::config::without_userinfo(proxy))
                })?;
                builder.proxy(p)
            };
            builder.build().map_err(|e| OnlineError::Io(e.to_string()))
        };
        Ok(Self {
            api: build(reqwest::redirect::Policy::none())?,
            files: build(reqwest::redirect::Policy::limited(5))?,
        })
    }

    fn read(
        url: &Url,
        response: reqwest::blocking::Response,
        limit: usize,
    ) -> Result<Response, OnlineError> {
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok());
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim().to_lowercase());
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(OnlineError::TooLarge {
                limit_mib: (limit >> 20) as u64,
            });
        }
        let mut body = Vec::new();
        response
            .take(limit as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|e| OnlineError::Unreachable {
                host: host_of(url),
                reason: e.to_string(),
            })?;
        if body.len() > limit {
            return Err(OnlineError::TooLarge {
                limit_mib: (limit >> 20) as u64,
            });
        }
        Ok(Response {
            status,
            body,
            retry_after,
            content_type,
        })
    }

    /// `GET url` on the API client.
    pub fn get(&self, url: &Url, headers: &[(&str, &str)]) -> Result<Response, OnlineError> {
        let mut request = self.api.get(url.clone());
        for (k, v) in headers {
            request = request.header(*k, *v);
        }
        let response = request.send().map_err(|e| failure(url, &e))?;
        Self::read(url, response, ANSWER_LIMIT)
    }

    /// `POST url` with a JSON body on the API client.
    pub fn post_json(
        &self,
        url: &Url,
        headers: &[(&str, &str)],
        body: &Value,
    ) -> Result<Response, OnlineError> {
        let mut request = self
            .api
            .post(url.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        for (k, v) in headers {
            request = request.header(*k, *v);
        }
        let response = request.send().map_err(|e| failure(url, &e))?;
        Self::read(url, response, ANSWER_LIMIT)
    }

    /// `GET url` of a file, at most [`FILE_LIMIT`] bytes.
    pub fn file(&self, url: &Url, headers: &[(&str, &str)]) -> Result<Response, OnlineError> {
        let mut request = self.files.get(url.clone());
        for (k, v) in headers {
            request = request.header(*k, *v);
        }
        let response = request.send().map_err(|e| failure(url, &e))?;
        Self::read(url, response, FILE_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-MET-003, T-MET-004: keys never travel in clear
    #[test]
    fn only_https_or_loopback() {
        assert!(checked_url("https://jimaku.cc/api").is_ok());
        assert!(checked_url("http://127.0.0.1:9/api").is_ok());
        assert!(checked_url("http://localhost:9/api").is_ok());
        for bad in ["http://jimaku.cc/api", "ftp://x", "", "jimaku.cc"] {
            assert!(checked_url(bad).is_err(), "{bad}");
        }
        let e = checked_url("http://user:secret@example.org").unwrap_err();
        assert!(!e.to_string().contains("secret"), "{e}");
    }

    #[test]
    fn statuses() {
        let r = |status, retry_after| Response {
            status,
            body: Vec::new(),
            retry_after,
            content_type: None,
        };
        assert!(r(200, None).ok("P").is_ok());
        assert_eq!(
            r(429, Some(7)).ok("P"),
            Err(OnlineError::RateLimited {
                provider: "P".into(),
                seconds: 7
            })
        );
        assert_eq!(
            r(401, None).ok("P"),
            Err(OnlineError::Unauthorized {
                provider: "P".into()
            })
        );
        assert_eq!(
            r(406, None).ok("P"),
            Err(OnlineError::Quota {
                provider: "P".into()
            })
        );
        assert_eq!(
            r(502, None).ok("P"),
            Err(OnlineError::Http {
                provider: "P".into(),
                status: 502
            })
        );
    }
}
