//! Test-only JSON adapter for Python/Rust text differential checks.
//!
//! The protocol is one JSON object on stdin and one JSON object on stdout.
//! Input fields are `text` plus optional `disable` and `enable` arrays with
//! the same meaning as their CLI flags; when both arrays are absent, normal
//! configuration discovery starts at the process cwd. Output fields are
//! `findings`, `fixed`, and `refixed`. Each finding contains `line`, `rule`,
//! `name`, `snippet`, and Rust's additional `range`; `range` is a half-open
//! pair of UTF-8 byte offsets within the checked line. Python's public Finding
//! has no coordinate field, so parity compares its four existing fields while
//! Rust's native tests retain responsibility for byte-range correctness.

use std::io::{self, Read, Write};

use limae::config::{CliOverrides, ConfigError, resolve};
use limae::directives::DirectiveError;
use limae::pipeline::{InitError, Pipeline};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Deserialize)]
struct Request {
    text: String,
    disable: Option<Vec<String>>,
    enable: Option<Vec<String>>,
}

#[derive(Serialize)]
struct Response {
    findings: Vec<Finding>,
    fixed: String,
    refixed: String,
}

#[derive(Serialize)]
struct Finding {
    line: usize,
    rule: &'static str,
    name: String,
    range: [usize; 2],
    snippet: String,
}

#[derive(Debug, Error)]
enum ProbeError {
    #[error("cannot read probe request: {0}")]
    Read(#[source] io::Error),
    #[error("invalid probe request: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("cannot determine probe cwd: {0}")]
    Cwd(#[source] io::Error),
    #[error("cannot resolve probe configuration: {0}")]
    Config(#[from] ConfigError),
    #[error("cannot initialize probe pipeline: {0}")]
    Init(#[from] InitError),
    #[error("cannot process probe text: {0}")]
    Directive(#[from] DirectiveError),
    #[error("cannot encode probe response: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("cannot write probe response: {0}")]
    Write(#[source] io::Error),
}

fn main() -> Result<(), ProbeError> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(ProbeError::Read)?;
    let request: Request = serde_json::from_str(&input).map_err(ProbeError::Decode)?;
    let cwd = std::env::current_dir().map_err(ProbeError::Cwd)?;
    let config = resolve(
        &cwd,
        CliOverrides {
            disable: request.disable.as_deref(),
            enable: request.enable.as_deref(),
        },
    )?;
    let pipeline = Pipeline::new()?;
    let findings = pipeline
        .check(&request.text, &config)?
        .into_iter()
        .map(|finding| Finding {
            line: finding.line,
            rule: finding.rule.as_str(),
            name: finding.name.into_owned(),
            range: [finding.range.start, finding.range.end],
            snippet: finding.snippet.to_owned(),
        })
        .collect();
    let fixed = pipeline.fix(&request.text, &config)?;
    let refixed = pipeline.fix(&fixed, &config)?;
    let response = Response {
        findings,
        fixed,
        refixed,
    };
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &response).map_err(ProbeError::Encode)?;
    writeln!(stdout).map_err(ProbeError::Write)?;
    stdout.flush().map_err(ProbeError::Write)
}
