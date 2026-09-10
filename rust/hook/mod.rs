//! The `hook` subcommand: one rewrite of what is about to be shown.
//!
//! `docs/adr/0009-polish-hook-contract.md` is the normative description of what
//! the hook does; the Python reference implementation this was ported from
//! has since been deleted.
//!
//! [`cli`] is the host protocol and the subcommand's entry point; the other
//! modules are the parts of the work that are not the protocol.

pub mod ab;
pub mod block;
pub mod cli;
pub mod parts;
pub mod render;
pub mod state;
