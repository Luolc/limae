//! The `hook` subcommand: one rewrite of what is about to be shown.
//!
//! `docs/adr/0009-polish-hook-contract.md` is the normative description of what
//! the hook does and `src/limae/hook.py` is the reference implementation; this
//! module holds the parts of it that are not the host protocol itself.

pub mod ab;
pub mod render;
pub mod state;
