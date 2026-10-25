# ADR 0008 — Online subtitles

| Field | Value |
|---|---|
| Status | Accepted |
| Date | 2026-10-25 |
| Requirements | MET-003, MET-004, MET-005, MET-007…MET-010, SUB-006, SUB-008, SET-007, PRV-001, NFR-SEC-02 |
| Milestone | M4b (decision D-14) |

## Context

Most videos a learner has carry no Japanese subtitles, and finding them by hand (search a site, download, unpack, rename next to the video) is the step where many give up. The specification planned online subtitles for M7, after the library (M5) and its series identification through AniList (MET-001). Decision D-14 moves them ahead, right after M4: searching and downloading Japanese subtitles, by hand from the player or automatically when a video has none.

Without M5 there is no series record and no AniList id. The reference implementation, Fushi (`jimaku_client.dart`, `open_subtitles_client.dart`, `video_filename_parser.dart`, commit `ac8034e`), searches Jimaku by AniList id first and by title as a fallback, and OpenSubtitles by file hash, then by external ids, then by title. This record fixes what Nikomiru does before M5, and how the result is kept so that a video needs the network only once.

## Decision

### Providers and requests

1. **Two providers, behind one trait** in `nikomiru-online`: **Jimaku** (MET-003), the main source of Japanese subtitles for anime and Japanese dramas, and **OpenSubtitles** (MET-004), for films and live action. Each needs the user's own key (Jimaku: an account's API key; OpenSubtitles: a consumer API key, and optionally a user name and password for the download quota). A provider without a key is not asked. AniList (MET-001) is not needed: Jimaku is searched by title, and by `anilist_id` once M7 identifies series.
2. **The search request** is built from the video: the title, season and episode parsed from the file name (§3), the OpenSubtitles hash and size of the file, and the study language (`[general] study_language`, `ja`). On the page (§11) the user can change the title, season and episode before searching.
3. **File names are parsed Anitomy-style**, a subset of Fushi's `FilenameParser` that LIB-003 (M5) will reuse: bracketed blocks (`[Group]`, `[ABCD1234]`, `(1080p)`) and technical tokens (resolution, codecs, sources, `v2`) are dropped; the episode is `SxxEyy`, `第N話/话/集/回`, `EP N`/`E N`, `#N`, ` - N`, or a trailing number of two digits or with a leading zero; the season is `SxxEyy`, `Season N`, `S N`, `Nth Season`, `第N期/季`, or comes from the parent folder when the name has an episode and no season; what is left before the episode is the title. Dots and underscores are word separators when the name has no spaces.
4. **Jimaku**: `GET /api/entries/search?query=<title>&anime=true`, then `anime=false` when nothing is found (Jimaku filters on `anime` and defaults it to `true`: without the second request dramas are never found, Fushi's BUG-1694); `GET /api/entries/<id>/files` for the first two entries, all their files (a season pack carries no episode in its name, so Jimaku's own `episode=` filter would hide it; the episode is matched here, §8); each file's `url` downloaded with the same `Authorization` header. The key is only ever sent to the configured Jimaku host: a file `url` on another host is refused.
5. **OpenSubtitles**: `GET /api/v1/subtitles` in tiers, the next one only when the previous finds nothing: `moviehash` + `moviebytesize`; then `query` + `season_number` + `episode_number`; always with `languages=<study language>`. `POST /api/v1/download {"file_id"}` gives a temporary link, which is fetched only over HTTPS. With a user name and password, `POST /api/v1/login` first, the token kept in memory for the run. Machine-translated results (`ai_translated`) are marked and never chosen automatically.
6. **HTTP** is reqwest's blocking client with native-tls, as for AnkiConnect (ADR 0007 §5): one request at a time on the `online` worker, 10 s to connect, 30 s per request, the `User-Agent` `Nikomiru/<version>`, the proxy of `[network] proxy` if set (SET-007) and never the environment's. Bodies are read with a limit: 4 MiB for an answer, 64 MiB for a download (MET-004). HTTP 429 becomes "too many requests, try again in N s" from `Retry-After`. Base URLs can be set in `[online]` for the tests; a base URL that is not HTTPS is accepted only on loopback, so a key never travels in clear.

### What is downloaded

7. **Formats**: SRT, ASS, SSA and WebVTT, which Nikomiru draws (§6.4). **ZIP archives** (season packs are common on Jimaku) are opened in memory with the rules of DIC-011: only file names, never paths, at most 64 MiB unpacked and 1000 entries; the subtitle files inside take part in the choice like the others. RAR and 7z need crates with unclear licenses or native code: they are listed with a note and cannot be used.
8. **Language and episode of a file** come from its name, as in Fushi: a language suffix (`ep01.ja.srt`, `[jpn]`), explicit markers (`日本語`, `[JP]`, `简体`), otherwise unknown; the episode with the parser of §3, after the extension and the language suffix are removed. On Jimaku a file of unknown language is taken as Japanese, which nearly all of them are.
9. **Ranking**: the preferred language first (a file of unknown language on Jimaku counts as it, §8; then unknown, then the others); the episode asked for first (then files without an episode, then other episodes, which are only listed); not machine-translated; text formats before archives; then the provider's own order (Jimaku's file list, OpenSubtitles' download count). The automatic choice (§10) takes the first file only if its language is the preferred one (or unknown on Jimaku) and its episode is the one asked for, or the video is a film.

### Where it goes

10. **Downloaded subtitles are kept in Nikomiru's data folder**, never next to the video (NFR-SEC-02): `$XDG_DATA_HOME/nikomiru/subtitles/<hash>/<video name>.<language>.<ext>`, where `<hash>` is the OpenSubtitles hash of the video (64 KiB from each end and the size), which survives renaming and moving the file. Every subtitle file in that folder is offered for the video like a file next to it (SUB-006), so the automatic choice (SUB-008) finds it the next time without the network. A name already taken gets ` (2)`.

### Automatic subtitles

11. **`[online] auto_subtitles`** (default off: the network is used only for features the user enables, PRV-001). When a video has been opened and SUB-008 found no text subtitles in the preferred language (no file next to it or downloaded, no embedded text track in that language), and a provider has a key, the session asks the `online` worker to search with the parsed name and the hash, and to download the best file of §9. When it arrives, the session loads it if the same video is still open and the user has not chosen subtitles meanwhile; a toast names the file and the provider. One attempt per opening of a video; nothing found is a quiet line in the log and a short toast.

### Interface

12. **"Search online…"** in the subtitle menu opens the **Online subtitles** page, over the player like the other pages: title, season and episode (filled from the file name), "Search", the results with provider, file name, language, episode and a note (machine translated, archive, not usable), "Use" to download and load one. The page also holds the online settings: the Jimaku key, the OpenSubtitles key, user name and password, automatic subtitles, the proxy. They are saved in `config.toml` with the atomic 0600 save (MET-005); the keys and the password never reach a log.
13. **The `online` worker** runs the requests one at a time on a thread of its own, like the `anki` worker (ADR 0007 §1): results reach the interface as messages; a search started for another video is answered but ignored; closing Nikomiru drops the queued jobs.

### Tests

14. A **fake HTTP server** in `nikomiru-online` behind the `test-support` feature, written like the fake AnkiConnect (ADR 0007 §7), answers both providers' endpoints from scripted data and records the requests. L1 vectors for the parser, the language detection, the hash and the ranking (the parser's from Fushi's tests); L4 for the clients and the archives; L6 for the automatic subtitles of a video without Japanese subtitles and for the page. Tests against the real services (T-MET-LIVE-*) need keys and run by hand.

## Consequences

- `nikomiru-online` depends on reqwest (already in the build for AnkiConnect) and on `zip` (already used by `nikomiru-dict`); no new license class.
- SPECIFICATION: MET-003 and MET-004 as above; new MET-007 (automatic subtitles), MET-008 (where they are kept), MET-009 (the page), MET-010 (the file-name parser); SUB-006 and SUB-008 include the downloaded files; SET-007 is `[network] proxy`; §9 gets `[online]` and `[network]`.
- ROADMAP: the new milestone M4b (D-14) holds these requirements and the mpv presets of PLY-016 and PLY-018, moved from M7 and M6.
- Downloaded subtitles are often timed for another release: once they load, they are aligned with the audio (SUB-018, also in the subtitle menu), and the offset is applied only when the match is clear.
- When M5 identifies series and M7 adds AniList, Jimaku is searched by `anilist_id` first, as in Fushi, and the downloaded subtitles stay where they are.
