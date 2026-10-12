#![cfg(unix)]

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};
use support::TestDir;

struct Sandbox {
    dir: TestDir,
}

impl Sandbox {
    fn new() -> Self {
        let sandbox = Self {
            dir: TestDir::new(),
        };
        for dir in [
            "styles",
            "fixtures/sample",
            "fixtures/other",
            "source",
            "tmp",
        ] {
            fs::create_dir_all(sandbox.dir.path().join(dir)).unwrap();
        }
        sandbox.write("styles/Sample.xml", "<code_scheme name=\"Sample\"/>");
        sandbox.write("source/Foo.java", "class Foo{}");
        sandbox.write("fixtures/sample/Foo.input.java", "original input");
        sandbox.write("fixtures/sample/Foo.expected.java", "original expected");
        sandbox.write("fixtures/other/Foo.input.java", "class Foo{}");
        sandbox.write(
            "formatter",
            r#"#!/usr/bin/env bash
set -eu
if [[ "${MOCK_FAIL:-0}" == 1 ]]; then
    echo 'IntelliJ: injected formatter failure' >&2
    exit 42
fi
target="${!#}"
if [[ -d "$target" ]]; then
    for file in "$target"/*.java; do
        printf 'formatted\n' > "$file"
    done
else
    printf 'formatted\n' > "$target"
fi
if [[ "${MOCK_VERBOSE:-0}" == 1 ]]; then
    for ((i=0; i<10000; i++)); do
        echo "Formatting file $i"
    done
fi
touch "$MOCK_COMPLETION"
"#,
        );
        fs::set_permissions(
            sandbox.dir.path().join("formatter"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        sandbox
    }

    fn write(&self, path: &str, contents: &str) {
        fs::write(self.dir.path().join(path), contents).unwrap();
    }

    fn run(&self, args: &[&str], fail: bool, verbose: bool) -> Output {
        Command::new("bash")
            .args([
                "-c",
                r#"source "$1"
FMT_PRIVATE="$2/styles"
FIXTURES_PRIVATE="$2/fixtures"
IDEA_FMT="$2/formatter"
shift 2
main "$@"
"#,
                "fixture-script-test",
            ])
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/fixtures.sh"))
            .arg(self.dir.path())
            .args(args)
            .current_dir(self.dir.path())
            .env("TMPDIR", self.dir.path().join("tmp"))
            .env("MOCK_FAIL", if fail { "1" } else { "0" })
            .env("MOCK_VERBOSE", if verbose { "1" } else { "0" })
            .env("MOCK_COMPLETION", self.dir.path().join("completed"))
            .output()
            .unwrap()
    }
}

fn assert_failure_preserves_pair(args: &[&str]) {
    let sandbox = Sandbox::new();
    let output = sandbox.run(args, true, false);
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("injected formatter failure"));
    for (suffix, original) in [
        ("input", "original input"),
        ("expected", "original expected"),
    ] {
        assert_eq!(
            fs::read_to_string(
                sandbox
                    .dir
                    .path()
                    .join(format!("fixtures/sample/Foo.{suffix}.java"))
            )
            .unwrap(),
            original,
        );
    }
    assert_eq!(
        fs::read_dir(sandbox.dir.path().join("tmp"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn add_failure_preserves_existing_pair() {
    assert_failure_preserves_pair(&["add", "source/Foo.java", "--force"]);
}

#[test]
fn project_failure_preserves_existing_pair() {
    assert_failure_preserves_pair(&["add-project", "source", "--force"]);
}

#[test]
fn seed_failure_preserves_existing_pair() {
    assert_failure_preserves_pair(&["seed", "sample", "--force"]);
}

#[test]
fn single_regeneration_failure_preserves_existing_pair() {
    assert_failure_preserves_pair(&["regen", "sample", "Foo"]);
}

#[test]
fn batch_regeneration_failure_preserves_existing_pair() {
    assert_failure_preserves_pair(&["regen", "sample"]);
}

#[test]
fn format_failure_preserves_existing_pair() {
    assert_failure_preserves_pair(&["fmt", "fixtures/sample/Foo.input.java", "styles/Sample.xml"]);
}

#[test]
fn quiet_formatter_success_writes_expected_output() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(
        &["fmt", "fixtures/sample/Foo.input.java", "styles/Sample.xml"],
        false,
        false,
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(sandbox.dir.path().join("fixtures/sample/Foo.expected.java")).unwrap(),
        "formatted\n",
    );
    assert_eq!(
        fs::read_dir(sandbox.dir.path().join("tmp"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn verbose_batch_formatter_finishes_without_broken_pipe() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["add-project", "source", "--force"], false, true);
    assert!(output.status.success(), "{output:?}");
    assert!(sandbox.dir.path().join("completed").is_file());
    assert_eq!(
        fs::read_to_string(sandbox.dir.path().join("fixtures/sample/Foo.expected.java")).unwrap(),
        "formatted\n",
    );
}

#[test]
fn successful_commands_publish_formatted_pairs() {
    for args in [
        vec!["add", "source/Foo.java", "--force"],
        vec!["add-project", "source", "--force"],
        vec!["seed", "sample", "--force"],
        vec!["regen", "sample", "Foo"],
        vec!["regen", "sample"],
    ] {
        let sandbox = Sandbox::new();
        let output = sandbox.run(&args, false, false);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert_eq!(
            fs::read_to_string(sandbox.dir.path().join("fixtures/sample/Foo.expected.java"))
                .unwrap(),
            "formatted\n",
        );
        let expected_input = if args[0] == "regen" {
            "original input"
        } else {
            "class Foo{}"
        };
        assert_eq!(
            fs::read_to_string(sandbox.dir.path().join("fixtures/sample/Foo.input.java")).unwrap(),
            expected_input,
        );
        assert_eq!(
            fs::read_dir(sandbox.dir.path().join("tmp"))
                .unwrap()
                .count(),
            0
        );
    }
}

#[test]
fn project_preserves_distinct_original_names() {
    let sandbox = Sandbox::new();
    sandbox.write("source/FooBar.java", "class FooBar{}");
    sandbox.write("source/Foo_bar.java", "class Foo_bar{}");
    let output = sandbox.run(&["add-project", "source", "--force"], false, false);
    assert!(output.status.success(), "{output:?}");
    for name in ["FooBar", "Foo_bar"] {
        assert!(
            sandbox
                .dir
                .path()
                .join(format!("fixtures/sample/{name}.expected.java"))
                .is_file()
        );
    }
}

#[test]
fn add_rejects_missing_styles_instead_of_succeeding_without_work() {
    let sandbox = Sandbox::new();
    fs::rename(
        sandbox.dir.path().join("styles"),
        sandbox.dir.path().join("saved-styles"),
    )
    .unwrap();
    let output = sandbox.run(&["add", "source/Foo.java"], false, false);
    assert!(!output.status.success(), "{output:?}");
}

#[test]
fn missing_batch_outputs_are_reported_as_failure() {
    for args in [
        vec!["add-project", "source", "--force"],
        vec!["seed", "sample", "--force"],
        vec!["regen", "sample"],
        vec!["add", "source/Foo.java", "--force"],
        vec!["regen", "sample", "Foo"],
        vec!["fmt", "fixtures/sample/Foo.input.java", "styles/Sample.xml"],
    ] {
        let sandbox = Sandbox::new();
        sandbox.write("formatter", "#!/usr/bin/env bash\nset -eu\ntarget=\"${!#}\"\nif [[ -d \"$target\" ]]; then\n mv -- \"$target/Foo.java\" \"$MOCK_COMPLETION\"\nelse\n mv -- \"$target\" \"$MOCK_COMPLETION\"\nfi\n");
        let output = sandbox.run(&args, false, false);
        assert!(!output.status.success(), "{args:?}: {output:?}");
        assert_eq!(
            fs::read_to_string(sandbox.dir.path().join("fixtures/sample/Foo.expected.java"))
                .unwrap(),
            "original expected"
        );
        assert_eq!(
            fs::read_to_string(sandbox.dir.path().join("fixtures/sample/Foo.input.java")).unwrap(),
            "original input"
        );
    }
}

#[test]
fn invalid_batch_sizes_are_rejected_even_in_dry_run() {
    for size in ["0", "-1", "invalid", "99999999999999999999999"] {
        let sandbox = Sandbox::new();
        let output = sandbox.run(
            &[
                "add-project",
                "source",
                "--dry-run",
                &format!("--batch-size={size}"),
            ],
            false,
            false,
        );
        assert!(!output.status.success(), "{size}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("batch size"),
            "{output:?}"
        );
    }
}

#[test]
fn incomplete_pairs_are_repaired_on_retry() {
    let sandbox = Sandbox::new();
    fs::rename(
        sandbox.dir.path().join("fixtures/sample/Foo.expected.java"),
        sandbox.dir.path().join("saved-expected"),
    )
    .unwrap();
    let output = sandbox.run(&["add", "source/Foo.java"], false, false);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(sandbox.dir.path().join("fixtures/sample/Foo.expected.java")).unwrap(),
        "formatted\n"
    );
}
