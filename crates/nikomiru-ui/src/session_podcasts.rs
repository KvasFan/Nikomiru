// SPDX-License-Identifier: GPL-3.0-only

//! Podcast feeds, downloads and timed text (POD-001…POD-004), as a child of
//! the session module.
use super::*;
use crate::podcasts::{Download, Fetched, Job};

/// A feed fetch to start: its epoch, address and cancellation.
pub type FetchRequest = (u64, String, nikomiru_core::Cancel);
/// Where a fetched feed goes: its epoch, address and result.
pub type FetchDone =
    Box<dyn FnOnce(u64, String, Result<nikomiru_online::podcast::Feed, String>) + Send>;
/// A download to start: its serial, feed and episode.
pub type DownloadRequest = (
    u64,
    String,
    nikomiru_library::podcasts::Episode,
    nikomiru_core::Cancel,
);

impl Session {
    /// Where downloaded episodes are kept: `<data>/podcasts`.
    pub fn podcasts_dir(&self) -> PathBuf {
        self.services.text_dir.with_file_name("podcasts")
    }

    /// The Podcasts page opened.
    pub fn podcasts_open(&mut self) {
        self.podcasts.open = true;
        self.close_all_popups();
        self.podcasts_request(Job::List);
    }

    /// Queue one library job; the writer runs them in order.
    pub fn podcasts_request(&mut self, job: Job) {
        if !matches!(job, Job::List) && !self.phrases_writable() {
            self.podcasts.error = Some(self.tr("phrases-readonly", &[]));
            return;
        }
        self.podcast_pending += 1;
        self.podcasts.busy = true;
        self.podcast_jobs.push(job);
    }

    /// The jobs to start.
    pub fn take_podcast_jobs(&mut self) -> Vec<Job> {
        std::mem::take(&mut self.podcast_jobs)
    }

    /// Run it through the serial writer.
    pub fn start_podcast_job(
        &self,
        job: Job,
        done: Box<dyn FnOnce(crate::podcasts::Answer) + Send>,
    ) {
        let dir = self.podcasts_dir();
        self.library_task(
            move |lib: &Library| crate::podcasts::run(lib, &dir, job),
            done,
        );
    }

    /// A job's answer.
    pub fn podcasts_done(&mut self, answer: crate::podcasts::Answer) {
        self.podcast_pending = self.podcast_pending.saturating_sub(1);
        self.podcasts.busy = self.podcast_pending > 0;
        match answer {
            Ok(feeds) => self.podcasts.feeds = feeds,
            Err(e) => {
                self.podcasts.error = Some(self.tr("podcasts-error", &[("reason", e)]));
            }
        }
    }

    /// Fetch a feed explicitly: a new one by its address, or a refresh.
    pub fn podcast_fetch(&mut self, url: &str) {
        if let Some((_, _, cancel)) = self.podcast_fetch_request.take() {
            cancel.cancel();
        }
        self.podcasts.error = None;
        self.podcasts.notice = None;
        if nikomiru_online::http::checked_url(url).is_err() {
            self.podcasts.error = Some(self.tr("podcasts-bad-url", &[]));
            return;
        }
        self.podcasts.epoch += 1;
        self.podcasts.fetching = Some(url.trim().to_string());
        self.podcast_fetch_request = Some((
            self.podcasts.epoch,
            url.trim().to_string(),
            nikomiru_core::Cancel::new(),
        ));
    }

    /// The fetch to start.
    pub fn take_podcast_fetch(&mut self) -> Option<FetchRequest> {
        self.podcast_fetch_request.take()
    }

    /// Fetch on a thread of its own.
    pub fn start_podcast_fetch(&self, (epoch, url, cancel): FetchRequest, done: FetchDone) {
        let proxy = self.services.config.network.proxy.clone();
        let spawned = std::thread::Builder::new()
            .name("podcast-feed".into())
            .spawn(move || {
                let result = crate::podcasts::fetch(&proxy, &url, &cancel);
                done(epoch, url, result);
            });
        if let Err(e) = spawned {
            tracing::error!("podcast feed: {e}");
        }
    }

    /// A fetched feed: merged into its record; a stale answer is dropped.
    pub fn podcast_fetched(
        &mut self,
        epoch: u64,
        url: String,
        result: Result<nikomiru_online::podcast::Feed, String>,
    ) {
        if epoch != self.podcasts.epoch || self.podcasts.fetching.as_deref() != Some(url.as_str()) {
            return;
        }
        self.podcasts.fetching = None;
        match result {
            Ok(parsed) => {
                let record = crate::podcasts::record(&url, parsed, self.phrase_now());
                self.podcasts_request(Job::Refresh(record));
            }
            Err(e) => self.podcasts.error = Some(self.tr("podcasts-error", &[("reason", e)])),
        }
    }

    /// Stop the fetch running.
    pub fn podcast_fetch_cancel(&mut self) {
        self.podcasts.epoch += 1;
        self.podcasts.fetching = None;
    }

    /// Download an episode explicitly, one at a time; a partial file resumes.
    pub fn podcast_download(&mut self, feed: &str, episode: &str) {
        if self.podcasts.download.is_some() || !self.phrases_writable() {
            return;
        }
        let Some((_, e)) = self.podcasts.episode(feed, episode) else {
            return;
        };
        let e = e.clone();
        self.podcasts.error = None;
        self.podcasts.notice = None;
        self.podcast_serial += 1;
        let cancel = nikomiru_core::Cancel::new();
        self.podcasts.download = Some(Download {
            feed: feed.into(),
            episode: episode.into(),
            done: 0,
            total: e.length,
            cancel: cancel.clone(),
        });
        self.podcast_download_request = Some((self.podcast_serial, feed.into(), e, cancel));
    }

    /// The download to start.
    pub fn take_podcast_download(&mut self) -> Option<DownloadRequest> {
        self.podcast_download_request.take()
    }

    /// The serial of the download running.
    pub fn podcast_serial(&self) -> u64 {
        self.podcast_serial
    }

    /// Download on a thread of its own; `progress` is called now and then.
    pub fn start_podcast_download(
        &self,
        (serial, feed, episode, cancel): DownloadRequest,
        progress: Box<dyn Fn(u64, u64, Option<u64>) + Send>,
        done: Box<dyn FnOnce(u64, Result<Fetched, String>) + Send>,
    ) {
        let proxy = self.services.config.network.proxy.clone();
        let dir = self.podcasts_dir();
        let spawned = std::thread::Builder::new()
            .name("podcast-download".into())
            .spawn(move || {
                let mut last = std::time::Instant::now();
                let result = crate::podcasts::download(
                    &proxy,
                    &dir,
                    &feed,
                    &episode,
                    &cancel,
                    &mut |d, t| {
                        if last.elapsed() > std::time::Duration::from_millis(200) {
                            last = std::time::Instant::now();
                            progress(serial, d, t);
                        }
                    },
                );
                done(serial, result);
            });
        if let Err(e) = spawned {
            tracing::error!("podcast download: {e}");
        }
    }

    /// How far the download is.
    pub fn podcast_progress(&mut self, serial: u64, done: u64, total: Option<u64>) {
        if serial == self.podcast_serial
            && let Some(d) = &mut self.podcasts.download
        {
            d.done = done;
            d.total = total.or(d.total);
        }
    }

    /// A download ended; recorded when it is the current one.
    pub fn podcast_downloaded(&mut self, serial: u64, result: Result<Fetched, String>) {
        if serial != self.podcast_serial {
            return;
        }
        let Some(d) = self.podcasts.download.take() else {
            return;
        };
        match result {
            Ok(fetched) => {
                let offered = self
                    .podcasts
                    .episode(&d.feed, &d.episode)
                    .is_some_and(|(_, e)| e.transcript.is_some());
                self.podcasts.notice = Some(match (&fetched.unsupported, &fetched.transcript) {
                    (Some(kind), _) => {
                        self.tr("podcasts-transcript-unsupported", &[("kind", kind.clone())])
                    }
                    (None, Some(_)) => self.tr("podcasts-transcript-saved", &[]),
                    (None, None) if offered => self.tr(
                        "podcasts-transcript-unsupported",
                        &[("kind", String::new())],
                    ),
                    (None, None) => self.tr("podcasts-transcript-none", &[]),
                });
                self.podcasts_request(Job::Downloaded {
                    feed: d.feed,
                    episode: d.episode,
                    file: fetched.file,
                    transcript: fetched.transcript,
                });
            }
            Err(e) => {
                let reason = if d.cancel.is_cancelled() {
                    self.tr("podcasts-cancelled", &[])
                } else {
                    e
                };
                self.podcasts.error =
                    Some(self.tr("podcasts-download-failed", &[("reason", reason)]));
                // the partial file is there to resume
                self.podcasts_request(Job::List);
            }
        }
    }

    /// Stop the download; its partial file stays.
    pub fn podcast_download_cancel(&mut self) {
        if let Some(d) = &self.podcasts.download {
            d.cancel.cancel();
        }
    }

    /// Whether an episode has a partial download.
    pub fn podcast_partial(
        &self,
        feed: &str,
        episode: &nikomiru_library::podcasts::Episode,
    ) -> bool {
        crate::podcasts::partial(&self.podcasts_dir(), feed, episode)
    }

    /// Play a downloaded episode, with its timed text beside it.
    pub fn podcast_play(&mut self, feed: &str, episode: &str) -> bool {
        let Some(file) = self
            .podcasts
            .episode(feed, episode)
            .and_then(|(_, e)| e.file.clone())
        else {
            return false;
        };
        let path = self.podcasts_dir().join(file);
        if !path.is_file() {
            self.podcasts.error = Some(self.tr("podcasts-missing-file", &[]));
            return false;
        }
        self.podcasts.open = false;
        self.open(&path);
        true
    }

    /// Open an episode's or a feed's web page, explicitly, outside.
    pub fn podcast_link(&mut self, url: &str) {
        if url.starts_with("https://") || url.starts_with("http://") {
            self.external_links.push(url.to_string());
        }
    }
}
