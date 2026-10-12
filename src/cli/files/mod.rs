mod discovery;
mod git;
mod write;

pub(crate) use discovery::collect_java_files;
pub(crate) use git::changed_files;
use write::atomic_write;

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use ijfmt::config::StyleConfig;
use ijfmt::formatter::{self, FormatOptions, LineSelection};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RunOptions<'a> {
    pub(crate) format: FormatOptions,
    pub(crate) check: bool,
    pub(crate) changed_base: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Success,
    WouldReformat,
    Failed,
}

impl Outcome {
    pub(crate) fn combine(self, other: Self) -> Self {
        use Outcome::*;
        match (self, other) {
            (Failed, _) | (_, Failed) => Failed,
            (WouldReformat, _) | (_, WouldReformat) => WouldReformat,
            (Success, Success) => Success,
        }
    }
}

pub(crate) fn run_single(
    path: &Path,
    config: &StyleConfig,
    options: RunOptions,
) -> Result<Outcome> {
    let target = resolve_file(path)?;
    let metadata =
        fs::metadata(&target).with_context(|| format!("Failed to inspect: {}", path.display()))?;
    let bytes = fs::read(&target).with_context(|| format!("Failed to read: {}", path.display()))?;
    let source = std::str::from_utf8(&bytes)
        .with_context(|| format!("Source is not valid UTF-8: {}", path.display()))?;

    let selection = match options.changed_base {
        Some(base) => git::changed_lines(&target, base)?,
        None => LineSelection::All,
    };
    let formatted = formatter::format_lines(source, config, options.format, &selection)
        .with_context(|| format!("Failed to format: {}", path.display()))?;

    if options.check {
        if formatted != source {
            eprintln!("{}: would reformat", path.display());
            return Ok(Outcome::WouldReformat);
        }
    } else if formatted != source {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() > 1 {
                bail!(
                    "Refusing to replace a file with multiple hard links: {}",
                    path.display()
                );
            }
        }
        if metadata.permissions().readonly() {
            bail!("Refusing to replace a read-only file: {}", path.display());
        }
        atomic_write(&target, formatted.as_bytes(), &metadata)
            .with_context(|| format!("Failed to write: {}", path.display()))?;
        eprintln!("{}: reformatted", path.display());
    }

    Ok(Outcome::Success)
}

pub(crate) fn run_files(files: &[PathBuf], config: &StyleConfig, options: RunOptions) -> Outcome {
    let mut seen = HashSet::new();
    let mut unique_files = Vec::new();
    let mut preparation = Outcome::Success;
    for path in files {
        let target = resolve_file(path).and_then(|target| {
            #[cfg(unix)]
            let identity = {
                use std::os::unix::fs::MetadataExt;
                let metadata = fs::metadata(&target)?;
                (metadata.dev(), metadata.ino())
            };
            #[cfg(not(unix))]
            let identity = target.clone();
            Ok((target, identity))
        });
        match target {
            Ok((target, identity)) => {
                if seen.insert(identity) {
                    unique_files.push(target);
                }
            }
            Err(error) => {
                eprintln!("error: {error:#}");
                preparation = Outcome::Failed;
            }
        }
    }
    let formatted = unique_files
        .par_iter()
        .map(|path| match run_single(path, config, options) {
            Ok(outcome) => outcome,
            Err(error) => {
                eprintln!("error: {error:#}");
                Outcome::Failed
            }
        })
        .reduce(|| Outcome::Success, Outcome::combine);
    preparation.combine(formatted)
}

fn resolve_file(path: &Path) -> Result<PathBuf> {
    let target =
        fs::canonicalize(path).with_context(|| format!("Failed to read: {}", path.display()))?;
    if !fs::metadata(&target)?.is_file() {
        bail!("Not a regular source file: {}", path.display());
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::Outcome::{self, *};

    #[test]
    fn batch_outcomes_prioritize_errors_over_changes() {
        assert_eq!(Success.combine(Success), Success);
        assert_eq!(Success.combine(WouldReformat), WouldReformat);
        assert_eq!(WouldReformat.combine(WouldReformat), WouldReformat);
        assert_eq!(WouldReformat.combine(Failed), Failed);
        assert_eq!(Success.combine(Failed), Failed);
    }

    #[test]
    fn batch_reduction_is_independent_of_parallel_order() {
        let outcomes = [Success, WouldReformat, Failed];
        for a in outcomes {
            assert_eq!(a.combine(Success), a);
            for b in outcomes {
                assert_eq!(a.combine(b), b.combine(a));
                for c in outcomes {
                    assert_eq!(a.combine(b).combine(c), a.combine(b.combine(c)));
                }
            }
        }
    }

    #[test]
    fn an_empty_batch_succeeds() {
        assert_eq!(
            super::run_files(
                &[],
                &ijfmt::config::StyleConfig::default(),
                super::RunOptions {
                    check: true,
                    ..Default::default()
                }
            ),
            Outcome::Success,
        );
    }

    #[test]
    fn discovery_reports_an_invalid_directory_root() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let (files, outcome) = super::collect_java_files(&root);
        assert!(files.is_empty());
        assert_eq!(outcome, Failed);
    }
}
