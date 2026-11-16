# ADR 0010 — Bounded document imports (M110)

Status: accepted for implementation under the whole-roadmap request, before adding format code.

## Decision

Import each format as inert reader paragraphs with the existing original-source identity and lookup/mining/indexing pipeline. Keep compressed input, decoded text, paragraph, nesting and process-output budgets explicit. Failed imports leave the open book intact.

- **HTMLZ**: reuse existing `zip`/Rust deflate and the safe HTML/ruby reader. Read the root `index.html` (or the only HTML document when unambiguous), never execute an embedded viewer or fetch its links/images.
- **DOCX**: reuse the graph's MIT `quick-xml` **0.41.0** as a direct workspace edge, with no new encoding/async/serialization feature. Read the supported WordprocessingML body, preserving paragraph/table order and literal text; reject DTD/entities, unsafe ZIP names and malformed structure. Do not load macros, external relationships, linked objects or embedded programs. See [Microsoft's document structure reference](https://learn.microsoft.com/en-us/office/open-xml/word/structure-of-a-wordprocessingml-document).
- **RTF**: an original bounded token reader, with inherited group state, Unicode/surrogate/fallback handling, ordinary paragraph/tab controls, ignored destinations and binary payload skipping. Decode legacy runs with the application's existing native `iconv`/Windows code-page adapter, injected through a format-support interface; do not add `encoding_rs`. Library-only callers retain Unicode/ASCII/Windows-1252 support and report unavailable legacy decoders explicitly. See [Microsoft's plain-text extraction rules](https://learn.microsoft.com/en-us/openspecs/exchange_server_protocols/ms-oxrtfex/205e1abf-b794-4fd0-b1e4-5210882233ab).
- **Text-layer PDF**: use Poppler's **GPL-2.0-or-later `pdftotext`** as an explicit native helper, with bounded original bytes passed through stdin, bounded UTF-8 stdout/stderr, a timeout and cancellation. Preserve page boundaries and embedded text stream order. This does not render pages, infer a visual reading order, run OCR, unlock encryption or fetch resources. Missing helpers and image-only/encrypted/invalid PDFs are explicit errors. Add setup inventory and explicit platform installation hooks; use signed distribution packages on supported Linux hosts, with their dependencies resolved by the existing package manager. Never install an unselected tool or elevate during ordinary document reading. Native Windows/macOS/Android installer/package/device validation remains under M117.

## License/dependency review

The existing policy in SPECIFICATION §3.2 and `deny.toml` explicitly bans `encoding_rs`; generic roadmap execution does not revoke that decision. A standalone, ignored `target/pdf-dependency-review` metadata probe of MIT `lopdf` 0.45.0 found newly required `encoding_rs`, `alloc-no-stdlib`, `alloc-stdlib` and `unicode-normalization` license exceptions. `pdf-extract` builds on lopdf; current pdf-rs also adds a Brotli allocation chain. Those paths are not selected. No exception or ban removal is proposed.

Poppler reuses the external GPL helper model already used for FFmpeg; the Rust workspace gains only the already resolved MIT XML parser edge. The helper is already installed on the available Linux machine. Source/format documentation and installed tool behaviour must be checked before promising extraction. The setup assistant must name a system-package install explicitly where it needs administrator authentication; denial/cancellation leaves import unavailable without modifying reading state. Sandboxed/native platform installers are tracked separately rather than treating cross-compilation as release validation.

## Persistence

Imported files remain user-owned files, as with EPUB; source bytes are not copied into library backups or WebDAV. Deterministic content identities and original page/chapter/paragraph byte locations support restart, phrase capture and sentence-bank invalidation. No database migration or new sync entity is required.
