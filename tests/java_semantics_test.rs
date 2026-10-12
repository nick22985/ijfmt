mod support;

use ijfmt::{config::StyleConfig, formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use support::TestDir;

fn java_tool(name: &str) -> PathBuf {
    std::env::var_os("IJFMT_JAVA_HOME")
        .map(|root| PathBuf::from(root).join("bin").join(name))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn checked_output(command: &mut Command) -> Output {
    let output = command
        .output()
        .expect("Install a JDK 17 or newer, or set IJFMT_JAVA_HOME");
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn compile_and_run(source: &str, root: &Path, name: &str) -> Vec<u8> {
    let directory = root.join(name);
    fs::create_dir(&directory).unwrap();
    let file = directory.join("Probe.java");
    fs::write(&file, source).unwrap();
    checked_output(
        Command::new(java_tool("javac"))
            .args(["-encoding", "UTF-8", "-d"])
            .arg(&directory)
            .arg(&file),
    );
    checked_output(
        Command::new(java_tool("java"))
            .arg("-cp")
            .arg(directory)
            .arg("Probe"),
    )
    .stdout
}

#[test]
#[ignore = "requires JDK 17 or newer; run by scripts/check.sh and CI"]
fn formatting_preserves_compiled_text_block_values_and_disabled_statements() {
    let dir = TestDir::new();
    let mut source = String::from("class Probe { public static void main(String[] args) {\n");
    for (i, value) in [
        "\u{a0}x",
        "\tx",
        "\t x\n  y",
        " x\\s",
        "\\t\\s",
        "\u{2007}x\n\u{202f}y",
    ]
    .iter()
    .enumerate()
    {
        source.push_str(&format!("String s{i} = \"\"\"\n{value}\n\"\"\";\nSystem.out.println(java.util.Arrays.toString(s{i}.codePoints().toArray()));\n"));
    }
    source.push_str(
        "// @formatter:off\nSystem.out.print(1); System.out.print(2);\n// @formatter:on\n} }\n",
    );
    let original = compile_and_run(&source, dir.path(), "original");
    for use_tabs in [false, true] {
        let cfg = StyleConfig {
            use_tabs,
            ..StyleConfig::default()
        };
        let formatted = formatter::format(&source, &cfg).unwrap();
        let name = if use_tabs { "tabs" } else { "spaces" };
        assert_eq!(
            compile_and_run(&formatted, dir.path(), name),
            original,
            "{name}: {formatted}"
        );
        assert_eq!(formatter::format(&formatted, &cfg).unwrap(), formatted);
    }
}
