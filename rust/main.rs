use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(limae::cli::run_process())
}
