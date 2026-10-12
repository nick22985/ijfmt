mod cli;

use anyhow::Result;
use std::process::ExitCode;

fn main() -> Result<ExitCode> {
    cli::run()
}
