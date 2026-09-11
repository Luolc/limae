//! The `hook` subcommand: this repository's deterministic typography fixes,
//! applied to each batch of an assistant reply as it is displayed.
//!
//! `docs/adr/0016-hook-mechanical-only.md` is the normative description.
//!
//! [`cli`] is the host protocol and the subcommand's entry point; [`parts`] is
//! the work on one batch — caching it, waiting for the batches before it, and
//! the prefix replay that fixes it; [`state`] is where the batches live between
//! processes and what is allowed to be in there.

pub mod cli;
pub mod parts;
pub mod state;
