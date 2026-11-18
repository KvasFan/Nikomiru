# ADR 0011 — Live device linking

| Field | Value |
|---|---|
| Status | **Proposed** (2026-11-18): waiting for the maintainer's decision (D-76) |
| Date | 2026-11-18 |
| Requirements | none yet; BAK-004…BAK-006 (WebDAV sync) stay as they are |
| Milestone | M126 |

## Context

The maintainer asked for an architecture proposal for linking devices (D-76). Today two Nikomiru installs share data only through WebDAV sync (BAK-004): progress, word statuses, review cards and media, merged when each one syncs. Nothing is live: one device cannot control another, and a phone cannot look a word up in what the computer is playing.

The roadmap's "live device linking" gap covers three wishes:

- **Remote and second screen**: play, pause, replay a line and look words up from the phone while the computer plays.
- **Hand-off**: stop on one device and go on at the same moment on another.
- **Streaming**: watch the computer's video on the phone.

§1.3 excludes cloud services, a headless server and iroh-style peer-to-peer networking. PRV-001 forbids network traffic the user did not ask for. NFR-SEC-01 forbids running content from outside.

## Proposal

### 1. Shape: the computer is the host, on the local network only

A computer running Nikomiru can **allow linking** (off by default). It then listens on one TCP port, on the local network interfaces only, never on a public address. Other devices are **clients**:

- first, a phone's **web browser**, which needs nothing installed and works before the Android port exists;
- later, the Android app (M117), speaking the same protocol.

There is no relay and no cloud. Away from home, the user's own VPN (WireGuard, Tailscale) makes the computer reachable like any local address, and Nikomiru does not need to know about it.

### 2. Pairing and trust

1. Turning linking on makes a long-lived **self-signed certificate** for the host (ECDSA P-256) and shows a **QR code** valid for five minutes. The QR code holds:
   - `https://<host-address>:<port>/pair`;
   - a 128-bit one-time token;
   - the certificate's SHA-256 fingerprint.
2. The phone opens the link and presents the token. The browser warns once about the self-signed certificate, and the user compares the fingerprint shown on both screens. The host then issues a **device credential**: a random 256-bit secret, stored as a hash, with a name and a creation date. The browser keeps it in its storage.
3. Every later request carries the credential. The device list in Settings shows each device with its last use and can **revoke** it. Turning linking off closes the port and keeps the list.

Without a valid token or credential, nothing answers beyond a fixed refusal. Pairing attempts are rate-limited, and the token works once.

### 3. Transport and protocol

- HTTPS for the page and static files. One **WebSocket** per client for live traffic: JSON messages of a versioned schema (`{"v":1,"type":…}`), each at most 64 KiB.
- **Client → host commands** (an allow-list; nothing else is accepted):
  - `play`, `pause`, `seek`, `replay_cue`, `next_cue`, `prev_cue`;
  - `lookup {text, offset}`;
  - `mine {entry}`;
  - `line_tool {translate|explain}`;
  - `status`.
- **Host → client events**:
  - `playback {title, position, paused}`, sent at most four times a second;
  - `cues {current, previous, next}` with plain text only;
  - `lookup_result`, as the popup's structured view model (`nikomiru_dict::structured`) as data, rendered by the client;
  - `mined {ok|error}`.
- Lookups and mining run **on the host**, with its dictionaries, AnkiConnect and settings. The phone needs no dictionary and no Anki of its own. HTML from dictionaries is never sent to be run: the client draws the same safe subset as the desktop popup (NFR-SEC-01).

### 4. Hand-off

When a linked client asks for `handoff`, the host saves progress at once through the same records as WebDAV sync (BAK-004) and pushes it, so there is one set of merge rules. A native client then opens the same item at that position. The browser page offers only "continue here" links to items the host can stream (phase 3).

### 5. Streaming (later)

The media-server relay (SRV-009) already serves a file over HTTP range requests on loopback. Phase 3 would offer it to paired clients over the same TLS port, behind the device credential. The client plays it with its own player (the browser's `<video>` element where the codec allows it). Transcoding is out of scope: incompatible files are refused with a reason.

### 6. Code layout

- `nikomiru-link` (new crate):
  - pairing and credential store (redb table `link_devices`);
  - the message schema and its validation;
  - the server loop on its own threads, like the other workers;
  - nothing interface-related.
- `nikomiru-ui`:
  - Settings → Linking (on/off, QR code, device list);
  - a `LinkService` trait like the other services;
  - inbound commands mapped to the existing `Command`s, so a remote action does exactly what the shortcut does.
- The phone page: static HTML/JS/CSS shipped in the binary. It has no framework and no third-party script, and it is translated from the same `.ftl` files at build time.

### 7. Dependencies (to check against §3.2 before any code)

| Need | Candidate | License | Note |
|---|---|---|---|
| TLS server | `rustls` with the `ring` provider | Apache-2.0 / MIT / ISC; `ring` has its own ISC-style license with OpenSSL-derived parts | `native-tls` cannot serve TLS portably. Alternatively, OpenSSL through `openssl`, already linked on Linux |
| Certificate | `rcgen` | MIT / Apache-2.0 | self-signed host certificate |
| WebSocket | `tungstenite` | MIT / Apache-2.0 | blocking, fits the worker threads |
| QR code | `qrcode` | MIT / Apache-2.0 | drawn by egui |
| Discovery (optional) | `mdns-sd` | MIT / Apache-2.0 | `_nikomiru._tcp`; the QR code makes it unnecessary for pairing |

## Phases

| Phase | Scope | Size |
|---|---|---|
| 1 | Pairing, device list, phone page: remote control, live lines, lookups and mining on the host | L |
| 2 | Hand-off of progress between linked installs (desktop ↔ desktop, then Android) | M |
| 3 | Streaming of compatible files to paired clients | L |

## Alternatives considered

- **Faster WebDAV sync**: simple, but nothing is live and every client needs a WebDAV server. It stays as the offline path.
- **A cloud relay**: excluded. There is no service to run (§1.3), and it would carry what the user watches through a third party.
- **iroh or other NAT-traversing peer-to-peer**: excluded by §1.3, large, and unnecessary when the user's own VPN reaches the host.
- **Android app only**: waits for M117 and leaves iOS and other phones out. A browser page works on any phone today, and the app can speak the same protocol later.

## Decisions asked of the maintainer

1. Accept the shape: the computer as host, LAN or the user's own VPN only, no cloud.
2. Accept the phone browser page as phase 1's client, before the Android app.
3. Choose the TLS stack: `rustls` with `ring`, or OpenSSL.
4. Accept the phases, or reorder them; for example, hand-off first.
