use std::io::Write;
use std::process::{Command, Stdio};

use ijfmt::config::StyleConfig;
use ijfmt::formatter::format;

#[test]
fn formatter_off_preserves_same_line_members_and_statements() {
    for source in [
        "class C {\n// @formatter:off\nint a; int b;\n// @formatter:on\n}\n",
        "class C { void f() {\n// @formatter:off\na(); b();\n// @formatter:on\n} }\n",
        "class C {\n// @formatter:off\n  int a;\t int b;  \n\n\n  int c;\n// @formatter:on\n}\n",
    ] {
        let disabled = source
            .split("@formatter:off")
            .nth(1)
            .unwrap()
            .split("// @formatter:on")
            .next()
            .unwrap();
        let formatted = format(source, &StyleConfig::default()).unwrap();
        assert!(formatted.contains(disabled), "{formatted:?}");
        assert_eq!(formatted.matches(disabled).count(), 1);
        assert_eq!(
            format(&formatted, &StyleConfig::default()).unwrap(),
            formatted
        );
    }
}

fn text_block_value(source: &str) -> String {
    let body = source.split("\"\"\"").nth(1).unwrap();
    let lines: Vec<_> = body.split('\n').skip(1).collect();
    let incidental = lines
        .iter()
        .filter(|line| !line.trim_matches([' ', '\t', '\u{c}']).is_empty())
        .chain(lines.last())
        .map(|line| {
            line.bytes()
                .take_while(|byte| matches!(byte, b' ' | b'\t' | 12))
                .count()
        })
        .min()
        .unwrap();
    lines
        .iter()
        .map(|line| {
            let prefix = line
                .bytes()
                .take_while(|byte| matches!(byte, b' ' | b'\t' | 12))
                .count();
            line[prefix.min(incidental)..].trim_end_matches([' ', '\t', '\u{c}'])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn text_blocks_preserve_significant_leading_characters() {
    for use_tabs in [false, true] {
        let cfg = StyleConfig {
            use_tabs,
            ..StyleConfig::default()
        };
        for body in [
            "\u{a0}x\n",
            "\tx\n",
            " \tx\n",
            "\u{a0}\nx\n",
            "    \tx\n    ",
            "\t x\n\t",
        ] {
            let source = format!("class C {{ String s = \"\"\"\n{body}\"\"\"; }}\n");
            let formatted = format(&source, &cfg).unwrap();
            assert_eq!(
                text_block_value(&formatted),
                text_block_value(&source),
                "{formatted:?}"
            );
            assert_eq!(format(&formatted, &cfg).unwrap(), formatted);
        }
    }
}

#[test]
fn excessive_nesting_does_not_abort_the_cli() {
    let source = format!(
        "class C {{ int f() {{ return {}1{}; }} }}\n",
        "(".repeat(4000),
        ")".repeat(4000)
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_ijfmt"))
        .args(["--style", "tests/fmt/Default.xml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.code().is_some()
            && !output.status.success()
            && String::from_utf8_lossy(&output.stderr).contains("nesting"),
        "status: {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn supported_nesting_formats_on_a_worker_stack() {
    if let Ok(depth) = std::env::var("IJFMT_TEST_WORKER_DEPTH") {
        let depth = depth.parse().unwrap();
        let shape = std::env::var("IJFMT_TEST_WORKER_SHAPE").unwrap();
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || {
                let source = match shape.as_str() {
                    "blocks" => format!(
                        "class C {{ void f() {{{}f();{}}} }}\n",
                        "{".repeat(depth),
                        "}".repeat(depth)
                    ),
                    "chains" => format!(
                        "class C {{ Object f() {{ return x{}; }} }}\n",
                        ".f()".repeat(depth)
                    ),
                    "parentheses" => format!(
                        "class C {{ int f() {{ return {}1{}; }} }}\n",
                        "(".repeat(depth),
                        ")".repeat(depth)
                    ),
                    _ => panic!("unknown nesting shape: {shape}"),
                };
                assert!(format(&source, &StyleConfig::default()).is_ok());
            })
            .unwrap()
            .join()
            .unwrap();
        return;
    }
    for (depth, shape) in [
        (32, "parentheses"),
        (64, "parentheses"),
        (128, "parentheses"),
        (256, "parentheses"),
        (500, "parentheses"),
        (128, "blocks"),
        (128, "chains"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "supported_nesting_formats_on_a_worker_stack",
                "--nocapture",
            ])
            .env("IJFMT_TEST_WORKER_DEPTH", depth.to_string())
            .env("IJFMT_TEST_WORKER_SHAPE", shape)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{shape} depth {depth}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
