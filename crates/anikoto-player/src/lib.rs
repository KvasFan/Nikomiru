// SPDX-License-Identifier: GPL-3.0-only

//! libmpv2 wrapper: initialization, observed properties, commands, events and
//! OpenGL rendering (milestone M1).
//!
//! It is the only workspace crate that depends on `libmpv2` (ARC-010). Together with
//! `anikoto-app::linux` it is also the only place where `unsafe` is allowed (ARC-011),
//! only for the GL callbacks and always with a `// SAFETY:` comment.

#![deny(missing_docs)]
