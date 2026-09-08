//! Rust implementation of limae's deterministic text processing.

pub mod cli;
pub mod config;
pub mod directives;
pub mod files;
pub mod hook;
pub mod markdown;
pub mod pipeline;
pub mod polish;
mod resources;
pub mod rules;
#[cfg(test)]
mod testing;
pub mod text;
