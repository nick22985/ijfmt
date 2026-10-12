use std::fs;
use std::path::{Path, PathBuf};

use super::Outcome;

pub(crate) fn collect_java_files(root: &Path) -> (Vec<PathBuf>, Outcome) {
    let mut files = Vec::new();
    let outcome = collect_java_files_inner(root, &mut files);
    (files, outcome)
}

fn collect_java_files_inner(dir: &Path, output: &mut Vec<PathBuf>) -> Outcome {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("error: Failed to read directory {}: {error}", dir.display());
            return Outcome::Failed;
        }
    };
    let mut outcome = Outcome::Success;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!("error: Failed to read entry in {}: {error}", dir.display());
                outcome = Outcome::Failed;
                continue;
            }
        };
        let path = entry.path();
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(error) => {
                eprintln!("error: Failed to inspect {}: {error}", path.display());
                outcome = Outcome::Failed;
                continue;
            }
        };
        if kind.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
            {
                continue;
            }
            outcome = outcome.combine(collect_java_files_inner(&path, output));
        } else if kind.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("java")
        {
            output.push(path);
        }
    }
    outcome
}
