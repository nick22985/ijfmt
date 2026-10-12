mod args;
mod files;
mod style;

use anyhow::{Context, Result};
use clap::Parser;
use ijfmt::{config, formatter};
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use args::Args;
use files::{Outcome, RunOptions};
use formatter::FormatOptions;

pub(crate) fn run() -> Result<ExitCode> {
    let args = Args::parse();
    let options = RunOptions {
        format: FormatOptions {
            malformed_java: args.malformed_java.into(),
        },
        check: args.check,
        changed_base: args.changed.as_deref(),
    };

    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let search_root = if let Some(file) = &args.file {
        cwd.join(file)
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| cwd.clone())
    } else {
        cwd.clone()
    };

    let style_path = style::resolve_style(args.style, args.fallback, &search_root, !args.check)?;
    let xml = fs::read_to_string(&style_path)
        .with_context(|| format!("Failed to read style file: {}", style_path.display()))?;
    let config = config::parse_style_xml(&xml)
        .with_context(|| format!("Failed to parse style XML: {}", style_path.display()))?;

    let outcome = if let Some(path) = args.file {
        files::run_single(&path, &config, options)?
    } else if args.project {
        let (paths, discovery) = match options.changed_base {
            Some(base) => (files::changed_files(&cwd, base)?, Outcome::Success),
            None => files::collect_java_files(&cwd),
        };
        discovery.combine(files::run_files(&paths, &config, options))
    } else if !args.files.is_empty() {
        files::run_files(&args.files, &config, options)
    } else {
        format_stdin(&config, options)?
    };

    Ok(match outcome {
        Outcome::Success => ExitCode::SUCCESS,
        Outcome::WouldReformat => ExitCode::from(1),
        Outcome::Failed => ExitCode::from(2),
    })
}

fn format_stdin(config: &config::StyleConfig, options: RunOptions) -> Result<Outcome> {
    let mut bytes = Vec::new();
    io::stdin()
        .read_to_end(&mut bytes)
        .context("Failed to read from stdin")?;
    let source = String::from_utf8_lossy(&bytes);
    let formatted = formatter::format_with_options(&source, config, options.format)
        .context("Failed to format stdin")?;

    if options.check && formatted != source {
        eprintln!("stdin: would reformat");
        return Ok(Outcome::WouldReformat);
    }
    if !options.check {
        io::stdout()
            .write_all(formatted.as_bytes())
            .context("Failed to write to stdout")?;
    }

    Ok(Outcome::Success)
}
