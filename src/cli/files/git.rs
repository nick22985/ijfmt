use anyhow::{Context, Result, bail};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;

use ijfmt::formatter::LineSelection;
pub(crate) fn changed_lines(path: &Path, base: &str) -> Result<LineSelection> {
    let dir = path.parent().context("Source has no parent directory")?;
    let name = path.file_name().context("Source has no file name")?;

    let untracked = git(dir, &["ls-files", "-z", "--others", "--"], name)?;
    if !untracked.is_empty() {
        return Ok(LineSelection::All);
    }

    let diff = git(
        dir,
        &[
            "diff",
            "-U0",
            "--no-color",
            "--no-ext-diff",
            "--no-renames",
            base,
            "--",
        ],
        name,
    )?;
    Ok(LineSelection::Ranges(parse_hunks(
        &String::from_utf8_lossy(&diff),
    )))
}
pub(crate) fn changed_files(root: &Path, base: &str) -> Result<Vec<PathBuf>> {
    let mut output = git(
        root,
        &[
            "diff",
            "-z",
            "--name-only",
            "--relative",
            "--no-renames",
            "--diff-filter=d",
            base,
            "--",
        ],
        ".",
    )?;
    output.extend(git(
        root,
        &["ls-files", "-z", "--others", "--exclude-standard", "--"],
        ".",
    )?);

    let mut files: Vec<PathBuf> = String::from_utf8_lossy(&output)
        .split('\0')
        .map(Path::new)
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("java"))
        .filter(|path| {
            !path
                .iter()
                .any(|component| component.to_str().is_some_and(|name| name.starts_with('.')))
        })
        .map(|path| root.join(path))
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

fn git(dir: &Path, args: &[&str], path: impl AsRef<std::ffi::OsStr>) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .arg(path)
        .output()
        .context("Failed to run git")?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args[0],
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}
fn parse_hunks(diff: &str) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for line in diff.lines() {
        let Some(header) = line.strip_prefix("@@ ") else {
            continue;
        };
        let Some(new) = header
            .split(' ')
            .nth(1)
            .and_then(|part| part.strip_prefix('+'))
        else {
            continue;
        };
        let (start, count) = match new.split_once(',') {
            Some((start, count)) => (start.parse::<usize>(), count.parse::<usize>()),
            None => (new.parse::<usize>(), Ok(1)),
        };
        let (Ok(start), Ok(count)) = (start, count) else {
            continue;
        };
        let range = if count == 0 {
            start.saturating_sub(1)..start + 1
        } else {
            start - 1..start - 1 + count
        };
        match ranges.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => ranges.push(range),
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::parse_hunks;

    fn spans(diff: &str) -> Vec<(usize, usize)> {
        parse_hunks(diff)
            .into_iter()
            .map(|range| (range.start, range.end))
            .collect()
    }

    #[test]
    fn hunks_become_zero_based_ranges() {
        let diff = "diff --git a/A.java b/A.java\n\
                    --- a/A.java\n\
                    +++ b/A.java\n\
                    @@ -3 +3 @@ class A {\n\
                    -a\n\
                    +b\n\
                    @@ -10,0 +11,2 @@\n\
                    +c\n\
                    +d\n";
        assert_eq!(spans(diff), [(2, 3), (10, 12)]);
    }

    #[test]
    fn deletions_mark_surrounding_lines() {
        assert_eq!(spans("@@ -5,2 +4,0 @@\n"), [(3, 5)]);
        assert_eq!(spans("@@ -1,2 +0,0 @@\n"), [(0, 1)]);
    }

    #[test]
    fn adjacent_hunks_merge() {
        assert_eq!(spans("@@ -1 +1 @@\n@@ -3,0 +2,0 @@\n"), [(0, 3)]);
    }
}
