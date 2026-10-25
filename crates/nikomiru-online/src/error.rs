// SPDX-License-Identifier: GPL-3.0-only

//! The errors of the online services, each with its message (ERR-001).

/// Why an online request failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OnlineError {
    /// The provider has no API key in the settings.
    #[error("{provider}: no API key")]
    NoKey {
        /// The provider.
        provider: String,
    },
    /// An address Nikomiru will not use: not HTTPS (outside loopback), or not
    /// the provider's host.
    #[error("{0} is not an address Nikomiru uses")]
    BadUrl(String),
    /// The connection failed.
    #[error("cannot reach {host}: {reason}")]
    Unreachable {
        /// The host.
        host: String,
        /// Why.
        reason: String,
    },
    /// No answer in time.
    #[error("no answer from {host} in time")]
    Timeout {
        /// The host.
        host: String,
    },
    /// The key, user name or password was refused (HTTP 401, 403).
    #[error("{provider} refused the key or the login")]
    Unauthorized {
        /// The provider.
        provider: String,
    },
    /// HTTP 429.
    #[error("{provider}: too many requests, try again in {seconds} s")]
    RateLimited {
        /// The provider.
        provider: String,
        /// When to try again.
        seconds: u64,
    },
    /// The download quota of the day is used up (OpenSubtitles HTTP 406).
    #[error("{provider}: the download quota is used up")]
    Quota {
        /// The provider.
        provider: String,
    },
    /// Another HTTP status.
    #[error("{provider}: HTTP {status}")]
    Http {
        /// The provider.
        provider: String,
        /// The status.
        status: u16,
    },
    /// An answer that is not what the provider sends.
    #[error("{provider}: unexpected answer ({reason})")]
    Protocol {
        /// The provider.
        provider: String,
        /// What is wrong.
        reason: String,
    },
    /// An answer or a file larger than the limit.
    #[error("the answer is larger than {limit_mib} MiB")]
    TooLarge {
        /// The limit, in MiB.
        limit_mib: u64,
    },
    /// A ZIP archive that cannot be read, or no subtitle file in it.
    #[error("the archive cannot be used: {0}")]
    Archive(String),
    /// A file that cannot be used: RAR, 7z, or not a subtitle.
    #[error("{0} cannot be used: only SRT, ASS, SSA, WebVTT and ZIP files")]
    Unusable(String),
    /// Reading or writing a file.
    #[error("{0}")]
    Io(String),
}

impl nikomiru_core::Localized for OnlineError {
    fn message_id(&self) -> &'static str {
        match self {
            OnlineError::NoKey { .. } => "error-online-no-key",
            OnlineError::BadUrl(_) => "error-online-bad-url",
            OnlineError::Unreachable { .. } => "error-online-unreachable",
            OnlineError::Timeout { .. } => "error-online-timeout",
            OnlineError::Unauthorized { .. } => "error-online-unauthorized",
            OnlineError::RateLimited { .. } => "error-online-rate-limited",
            OnlineError::Quota { .. } => "error-online-quota",
            OnlineError::Http { .. } => "error-online-http",
            OnlineError::Protocol { .. } => "error-online-protocol",
            OnlineError::TooLarge { .. } => "error-online-too-large",
            OnlineError::Archive(_) => "error-online-archive",
            OnlineError::Unusable(_) => "error-online-unusable",
            OnlineError::Io(_) => "error-online-io",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            OnlineError::NoKey { provider }
            | OnlineError::Unauthorized { provider }
            | OnlineError::Quota { provider } => vec![("provider", provider.clone())],
            OnlineError::BadUrl(url) => vec![("url", url.clone())],
            OnlineError::Unreachable { host, reason } => {
                vec![("host", host.clone()), ("reason", reason.clone())]
            }
            OnlineError::Timeout { host } => vec![("host", host.clone())],
            OnlineError::RateLimited { provider, seconds } => {
                vec![
                    ("provider", provider.clone()),
                    ("seconds", seconds.to_string()),
                ]
            }
            OnlineError::Http { provider, status } => {
                vec![
                    ("provider", provider.clone()),
                    ("status", status.to_string()),
                ]
            }
            OnlineError::Protocol { provider, reason } => {
                vec![("provider", provider.clone()), ("reason", reason.clone())]
            }
            OnlineError::TooLarge { limit_mib } => vec![("limit", limit_mib.to_string())],
            OnlineError::Archive(reason) | OnlineError::Io(reason) => {
                vec![("reason", reason.clone())]
            }
            OnlineError::Unusable(name) => vec![("name", name.clone())],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-ERR-001
    #[test]
    fn every_error_has_its_message() {
        let p = || "Jimaku".to_string();
        let errors = [
            OnlineError::NoKey { provider: p() },
            OnlineError::BadUrl("http://x".into()),
            OnlineError::Unreachable {
                host: "h".into(),
                reason: "r".into(),
            },
            OnlineError::Timeout { host: "h".into() },
            OnlineError::Unauthorized { provider: p() },
            OnlineError::RateLimited {
                provider: p(),
                seconds: 3,
            },
            OnlineError::Quota { provider: p() },
            OnlineError::Http {
                provider: p(),
                status: 500,
            },
            OnlineError::Protocol {
                provider: p(),
                reason: "r".into(),
            },
            OnlineError::TooLarge { limit_mib: 64 },
            OnlineError::Archive("r".into()),
            OnlineError::Unusable("a.rar".into()),
            OnlineError::Io("r".into()),
        ];
        let refs: Vec<&dyn nikomiru_core::Localized> = errors
            .iter()
            .map(|e| e as &dyn nikomiru_core::Localized)
            .collect();
        let i18n = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/i18n");
        let problems = nikomiru_core::error::message_problems(&i18n, &refs);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
