// SPDX-License-Identifier: GPL-3.0-only

//! Yomitan dictionaries: safe import, redb storage, deinflection, lookup and
//! word audio (milestones M3 and M5).
//!
//! Pure logic: no `dioxus`, `gtk` or `libmpv2` (ARC-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
