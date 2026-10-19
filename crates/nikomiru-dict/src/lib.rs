// SPDX-License-Identifier: GPL-3.0-only

//! Yomitan dictionaries: safe import, redb storage, deinflection, lookup and
//! word audio (milestones M3 and M5).
//!
//! Pure logic: no Qt and no `libmpv2` (ARC-010). It knows nothing about video,
//! so books and manga can use it after 1.0 (D-12).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod import;
pub mod store;
pub mod yomitan;
