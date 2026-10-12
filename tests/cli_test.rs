mod support;

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use support::TestDir;

const SOURCE: &str = "class A{int x=1;}\n";
const FORMATTED: &str = "class A\n{\n\tint x = 1;\n}\n";

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ijfmt"));
    command
        .arg("--style")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fmt/Sample.xml"));
    command
}

fn stdin_output(source: &[u8], check: bool) -> Output {
    let mut command = command();
    if check {
        command.arg("--check");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(source).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn stdin_formats_and_check_reports_changes_without_output() {
    let formatted = stdin_output(SOURCE.as_bytes(), false);
    assert!(formatted.status.success());
    assert_eq!(formatted.stdout, FORMATTED.as_bytes());

    let changed = stdin_output(SOURCE.as_bytes(), true);
    assert_eq!(changed.status.code(), Some(1));
    assert!(changed.stdout.is_empty());

    let unchanged = stdin_output(FORMATTED.as_bytes(), true);
    assert!(unchanged.status.success());
    assert!(unchanged.stdout.is_empty());
}

#[test]
fn stdin_preserves_crlf_and_decodes_invalid_utf8() {
    let source = b"class A{String s=\"\xff\";}\r\n";
    let output = stdin_output(source, false);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "class A\r\n{\r\n\tString s = \"\u{fffd}\";\r\n}\r\n",
    );
}

#[test]
fn file_check_does_not_write_and_formatting_is_idempotent() {
    let dir = TestDir::new();
    let path = dir.path().join("A.java");
    fs::write(&path, SOURCE).unwrap();

    let checked = command()
        .arg("--check")
        .arg("--file")
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(checked.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);

    assert!(
        command()
            .arg("--file")
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), FORMATTED);
    assert!(
        command()
            .arg("--check")
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn batch_processes_readable_files_even_when_another_file_fails() {
    let dir = TestDir::new();
    let path = dir.path().join("A.java");
    fs::write(&path, SOURCE).unwrap();

    let output = command()
        .arg(dir.path().join("missing.java"))
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&path).unwrap(), FORMATTED);
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing.java"));
}

#[test]
fn batch_check_leaves_files_unchanged_and_prioritizes_errors() {
    let dir = TestDir::new();
    let paths = [dir.path().join("A.java"), dir.path().join("B.java")];
    for path in &paths {
        fs::write(path, SOURCE).unwrap();
    }

    let output = command().arg("--check").args(&paths).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    for path in &paths {
        assert_eq!(fs::read_to_string(path).unwrap(), SOURCE);
    }

    let failed = command()
        .arg("--check")
        .args(&paths)
        .arg(dir.path().join("missing.java"))
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(2));
    for path in &paths {
        assert_eq!(fs::read_to_string(path).unwrap(), SOURCE);
    }
}

#[test]
fn single_file_errors_keep_exit_code_one() {
    let dir = TestDir::new();
    let output = command()
        .arg("--file")
        .arg(dir.path().join("missing.java"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to read"));
}

#[test]
fn successful_batch_formats_every_file_and_then_checks_clean() {
    let dir = TestDir::new();
    let paths = [dir.path().join("A.java"), dir.path().join("B.java")];
    for path in &paths {
        fs::write(path, SOURCE).unwrap();
    }
    let output = command().args(&paths).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    for path in &paths {
        assert_eq!(fs::read_to_string(path).unwrap(), FORMATTED);
    }
    let checked = command().arg("--check").args(&paths).output().unwrap();
    assert!(checked.status.success(), "{checked:?}");
}

#[test]
fn boundary_invalid_utf8_is_rejected_without_changing_source() {
    let dir = TestDir::new();
    let path = dir.path().join("A.java");
    let bytes = b"class A{String s=\"caf\xe9\";}\n";
    fs::write(&path, bytes).unwrap();
    for check in [false, true] {
        let mut cmd = command();
        if check {
            cmd.arg("--check");
        }
        let output = cmd.arg("--file").arg(&path).output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(String::from_utf8_lossy(&output.stderr).contains("UTF-8"));
    }
}

fn isolated_style_command(dir: &TestDir) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ijfmt"));
    cmd.env("HOME", dir.path().join("home"))
        .env("XDG_CONFIG_HOME", dir.path().join("config"));
    cmd
}

#[test]
fn boundary_check_fallback_does_not_persist_style_configuration() {
    let dir = TestDir::new();
    let project = dir.path().join("project");
    let idea_styles = dir
        .path()
        .join("home/.config/JetBrains/IdeaIC2026.1/codestyles");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::create_dir_all(&idea_styles).unwrap();
    let path = project.join("A.java");
    fs::write(&path, SOURCE).unwrap();
    let fallback = dir.path().join("Boundary.xml");
    fs::write(&fallback, "<code_scheme name=\"Boundary\" />").unwrap();
    let output = isolated_style_command(&dir)
        .current_dir(&project)
        .arg("--fallback")
        .arg(&fallback)
        .arg("--check")
        .arg("--file")
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("would reformat"));
    assert!(!project.join(".idea").exists());
    assert!(!idea_styles.join("Boundary.xml").exists());
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);
    let formatted = isolated_style_command(&dir)
        .current_dir(&project)
        .arg("--fallback")
        .arg(&fallback)
        .arg("--file")
        .arg(&path)
        .output()
        .unwrap();
    assert!(formatted.status.success(), "{formatted:?}");
    assert!(
        project
            .join(".idea/codeStyles/codeStyleConfig.xml")
            .is_file()
    );
    assert!(idea_styles.join("Boundary.xml").is_file());
}

#[test]
fn boundary_relative_file_finds_the_same_ancestor_style_as_absolute_file() {
    let dir = TestDir::new();
    let project = dir.path().join("project");
    let subdir = project.join("src");
    let config = dir.path().join("config/ijfmt/styles");
    fs::create_dir_all(&subdir).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(project.join(".idea/codeStyles")).unwrap();
    fs::write(
        config.join("Boundary.xml"),
        "<code_scheme name=\"Boundary\" />",
    )
    .unwrap();
    fs::write(project.join(".idea/codeStyles/codeStyleConfig.xml"),
        "<component><option name=\"PREFERRED_PROJECT_CODE_STYLE\" value=\"Boundary\" /></component>").unwrap();
    let path = subdir.join("A.java");
    fs::write(&path, SOURCE).unwrap();
    for input in [path, PathBuf::from("A.java"), PathBuf::from("./A.java")] {
        let output = isolated_style_command(&dir)
            .current_dir(&subdir)
            .arg("--check")
            .arg("--file")
            .arg(&input)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("would reformat"),
            "{output:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn boundary_project_skips_file_and_directory_symlinks() {
    use std::os::unix::fs::symlink;
    let dir = TestDir::new();
    let project = dir.path().join("project");
    let outside = dir.path().join("outside");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let external = outside.join("Outside.java");
    fs::write(&external, SOURCE).unwrap();
    fs::write(project.join("Local.java"), SOURCE).unwrap();
    symlink(&outside, project.join("linked")).unwrap();
    symlink(&external, project.join("Alias.java")).unwrap();
    symlink(&project, project.join("cycle")).unwrap();
    symlink(outside.join("Missing.java"), project.join("Broken.java")).unwrap();
    let output = command()
        .current_dir(&project)
        .arg("--project")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(&external).unwrap(), SOURCE);
    assert_eq!(
        fs::read_to_string(project.join("Local.java")).unwrap(),
        FORMATTED
    );
    assert!(
        command()
            .current_dir(&project)
            .arg("--project")
            .arg("--check")
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[cfg(unix)]
#[test]
fn boundary_explicit_aliases_are_checked_once_and_symlinks_survive_formatting() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    let dir = TestDir::new();
    let path = dir.path().join("A.java");
    let alias = dir.path().join("Alias.java");
    fs::write(&path, SOURCE).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let before = fs::metadata(&path).unwrap();
    symlink(&path, &alias).unwrap();
    let output = command()
        .arg("--check")
        .arg(&path)
        .arg(&path)
        .arg(&alias)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("would reformat")
            .count(),
        1
    );
    let formatted = command().arg("--file").arg(&alias).output().unwrap();
    assert!(formatted.status.success(), "{formatted:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), FORMATTED);
    assert!(
        fs::symlink_metadata(&alias)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let after = fs::metadata(&path).unwrap();
    assert_eq!((after.uid(), after.gid()), (before.uid(), before.gid()));
}

#[cfg(unix)]
#[test]
fn boundary_hardlinks_are_checked_once_and_refused_for_changed_in_place_writes() {
    let dir = TestDir::new();
    let path = dir.path().join("A.java");
    let alias = dir.path().join("Alias.java");
    fs::write(&path, SOURCE).unwrap();
    fs::hard_link(&path, &alias).unwrap();
    let checked = command()
        .arg("--check")
        .arg(&path)
        .arg(&alias)
        .output()
        .unwrap();
    assert_eq!(checked.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&checked.stderr)
            .matches("would reformat")
            .count(),
        1
    );
    let output = command().arg("--file").arg(&path).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("hard link"));
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);
    assert_eq!(fs::read_to_string(&alias).unwrap(), SOURCE);
}

#[cfg(unix)]
#[test]
fn boundary_project_reports_unreadable_directories_and_continues() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestDir::new();
    let restricted = dir.path().join("restricted");
    fs::create_dir(&restricted).unwrap();
    fs::write(restricted.join("Hidden.java"), SOURCE).unwrap();
    let readable = dir.path().join("Readable.java");
    fs::write(&readable, SOURCE).unwrap();
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o000)).unwrap();
    let unreadable = fs::read_dir(&restricted).is_err();
    let output = command().current_dir(dir.path()).arg("--project").output();
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o700)).unwrap();
    if !unreadable {
        eprintln!("directory permission denial is unavailable for this account");
        return;
    }
    let output = output.unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("restricted"));
    assert_eq!(fs::read_to_string(&readable).unwrap(), FORMATTED);
}

#[cfg(unix)]
#[test]
fn boundary_failed_write_preserves_original_source_and_removes_temporary_file() {
    let dir = TestDir::new();
    let path = dir.path().join("A.java");
    let source = format!("class A{{String s=\"{}\";}}\n", "x".repeat(4096));
    fs::write(&path, &source).unwrap();
    let output = Command::new("bash")
        .args([
            "-c",
            "trap '' XFSZ; ulimit -f 1; exec \"$@\"",
            "boundary-test",
        ])
        .arg(env!("CARGO_BIN_EXE_ijfmt"))
        .arg("--style")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fmt/Sample.xml"))
        .arg("--file")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn boundary_interrupted_write_preserves_original_source() {
    use std::os::unix::process::ExitStatusExt;
    let dir = TestDir::new();
    let path = dir.path().join("A.java");
    let source = format!("class A{{String s=\"{}\";}}\n", "x".repeat(4096));
    fs::write(&path, &source).unwrap();
    let output = Command::new("bash")
        .args(["-c", "ulimit -f 1; exec \"$@\"", "boundary-test"])
        .arg(env!("CARGO_BIN_EXE_ijfmt"))
        .arg("--style")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fmt/Sample.xml"))
        .arg("--file")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.signal().is_some(), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}

#[cfg(unix)]
#[test]
fn boundary_failed_temporary_file_creation_preserves_source() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestDir::new();
    let path = dir.path().join("A.java");
    fs::write(&path, SOURCE).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let probe = dir.path().join("permission-probe");
    let creation_denied = fs::File::create(&probe).is_err();
    let output = command().arg("--file").arg(&path).output();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    if !creation_denied {
        eprintln!("directory write denial is unavailable for this account");
        return;
    }
    let output = output.unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn boundary_atomic_replacement_does_not_bypass_readonly_source_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestDir::new();
    let path = dir.path().join("A.java");
    fs::write(&path, SOURCE).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    let output = command().arg("--file").arg(&path).output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o400
    );
}
