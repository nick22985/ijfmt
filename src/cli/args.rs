use clap::{ArgGroup, Parser, ValueEnum};
use std::path::PathBuf;

use ijfmt::formatter;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum MalformedJavaArg {
    Preserve,
    Repair,
}

impl From<MalformedJavaArg> for formatter::MalformedJavaMode {
    fn from(value: MalformedJavaArg) -> Self {
        match value {
            MalformedJavaArg::Preserve => Self::Preserve,
            MalformedJavaArg::Repair => Self::Repair,
        }
    }
}

/// Fast Java formatter that reads IntelliJ code style XML files.
///
/// Modes (pick one):
///   (default)   Read from stdin, write formatted source to stdout.
///   --file      Format a single file in-place.
///   --project   Walk from the current directory and format every *.java file.
///   [files...]  Format the listed files in-place (legacy positional form).
#[derive(Parser, Debug)]
#[command(name = "ijfmt", version, about)]
#[command(group = ArgGroup::new("input").args(["file", "project", "files"]).multiple(true))]
pub(crate) struct Args {
    /// Path to IntelliJ code style XML file (e.g. Sample.xml).
    /// If omitted, ijfmt walks up from the current directory (or the directory
    /// of --file) looking for .idea/codeStyles/codeStyleConfig.xml, then falls
    /// back to `style = <name>` in ~/.config/ijfmt/config. Either way the name
    /// is resolved against ~/.config/ijfmt/styles/ and then
    /// ~/.config/JetBrains/<IdeaIC*|IntelliJIdea*>/codestyles/.
    #[arg(short, long, value_name = "FILE")]
    pub(crate) style: Option<PathBuf>,

    /// Fallback style XML to use when no project style is configured.
    /// If the project has no .idea/codeStyles set up, this style is used and
    /// the project is configured to use it for future runs, except during --check.
    #[arg(long, value_name = "FILE")]
    pub(crate) fallback: Option<PathBuf>,

    /// Format a single file in-place. The file's directory is used as the
    /// starting point for .idea style resolution. Intended for editor integration.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["project", "files"])]
    pub(crate) file: Option<PathBuf>,

    /// Format every *.java file found under the current directory (recursive).
    /// Skips hidden directories and symbolic links.
    #[arg(long, conflicts_with_all = ["file", "files"])]
    pub(crate) project: bool,

    /// Check mode: exit non-zero if any file would be reformatted. No files are written.
    #[arg(short, long)]
    pub(crate) check: bool,

    /// Handling for recoverable malformed Java: preserve malformed constructs
    /// or remove duplicate constructs before formatting.
    #[arg(long, value_enum, default_value = "preserve")]
    pub(crate) malformed_java: MalformedJavaArg,

    /// Only reformat lines that differ from a git revision (default HEAD), including
    /// staged and unstaged edits. Untracked files are formatted in full. With
    /// --project, only changed files are visited. Not available for stdin.
    #[arg(long, value_name = "REF", num_args = 0..=1, require_equals = true, default_missing_value = "HEAD", requires = "input")]
    pub(crate) changed: Option<String>,

    /// Java source files to format in-place. If omitted and --file / --project are
    /// not given, reads from stdin and writes to stdout.
    pub(crate) files: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_java_defaults_to_preserve() {
        let args = Args::try_parse_from(["ijfmt"]).unwrap();

        assert_eq!(args.malformed_java, MalformedJavaArg::Preserve);
    }

    #[test]
    fn malformed_java_accepts_repair() {
        let args = Args::try_parse_from(["ijfmt", "--malformed-java", "repair"]).unwrap();

        assert_eq!(args.malformed_java, MalformedJavaArg::Repair);
    }

    #[test]
    fn changed_defaults_to_head_without_consuming_files() {
        let args = Args::try_parse_from(["ijfmt", "--changed", "A.java"]).unwrap();
        assert_eq!(args.changed.as_deref(), Some("HEAD"));
        assert_eq!(args.files, [PathBuf::from("A.java")]);

        let args = Args::try_parse_from(["ijfmt", "--changed=main", "A.java"]).unwrap();
        assert_eq!(args.changed.as_deref(), Some("main"));
    }

    #[test]
    fn changed_requires_a_file_input() {
        assert_eq!(
            Args::try_parse_from(["ijfmt", "--changed"])
                .unwrap_err()
                .kind(),
            clap::error::ErrorKind::MissingRequiredArgument,
        );
        for args in [
            vec!["ijfmt", "--changed", "--file", "A.java"],
            vec!["ijfmt", "--changed", "--project"],
        ] {
            assert!(Args::try_parse_from(args).is_ok());
        }
    }

    #[test]
    fn input_modes_are_mutually_exclusive() {
        for args in [
            vec!["ijfmt", "--file", "A.java", "--project"],
            vec!["ijfmt", "--file", "A.java", "B.java"],
            vec!["ijfmt", "--project", "A.java"],
        ] {
            assert_eq!(
                Args::try_parse_from(args).unwrap_err().kind(),
                clap::error::ErrorKind::ArgumentConflict,
            );
        }
    }
}
