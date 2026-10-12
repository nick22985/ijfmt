use ijfmt::{config, formatter};
use libtest_mimic::{Arguments, Failed, Trial};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

#[path = "support/fixture_discovery.rs"]
mod fixture_discovery;

fn main() {
    let args = Arguments::from_args();
    let mut trials = Vec::new();

    trials.push(Trial::test(
        "no_unrecognized_options_in_style_xmls",
        check_no_unrecognized_options,
    ));

    let fixtures = collect_fixture_trials().unwrap_or_else(|error| {
        eprintln!("Fixture discovery failed: {error}");
        std::process::exit(1);
    });
    trials.extend(fixtures);

    libtest_mimic::run(&args, trials).exit();
}

struct FixtureStyle {
    xml_path: PathBuf,
    config: OnceLock<Result<config::StyleConfig, String>>,
}

impl FixtureStyle {
    fn config(&self) -> Result<&config::StyleConfig, Failed> {
        self.config
            .get_or_init(|| {
                let xml = fs::read_to_string(&self.xml_path).map_err(|e| {
                    format!("Cannot read style XML {}: {e}", self.xml_path.display())
                })?;
                config::parse_style_xml(&xml)
                    .map_err(|e| format!("Cannot parse style XML {}: {e}", self.xml_path.display()))
            })
            .as_ref()
            .map_err(|e| e.clone().into())
    }
}

fn collect_fixture_trials() -> Result<Vec<Trial>, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut style_overrides = std::collections::HashMap::from([(
        "sample::tmp/TestAlignment",
        root.join("tests/fmt/Default.xml"),
    )]);
    let pairs = [
        ("tests/fmt", "tests/fixtures"),
        ("tests/fmt/private", "tests/fixtures/private"),
    ];

    let mut trials = Vec::new();

    for (index, (xml_dir, fixtures_base)) in pairs.iter().enumerate() {
        let xml_dir = root.join(xml_dir);
        let corpus = root.join(fixtures_base);
        let discovered = if index == 0 {
            fixture_discovery::discover(&xml_dir, &corpus)?
        } else {
            fixture_discovery::discover_optional(&xml_dir, &corpus)?
        };
        for discovered in discovered {
            eprintln!(
                "fixtures: {}: {} cases",
                discovered.name,
                discovered.fixtures.len()
            );
            let style = Arc::new(FixtureStyle {
                xml_path: discovered.xml,
                config: OnceLock::new(),
            });
            for fixture in discovered.fixtures {
                let name = format!("{}::{}", discovered.name, fixture.name);
                let style = if let Some(xml_path) = style_overrides.remove(name.as_str()) {
                    eprintln!("fixture style override: {name}: {}", xml_path.display());
                    Arc::new(FixtureStyle {
                        xml_path,
                        config: OnceLock::new(),
                    })
                } else {
                    Arc::clone(&style)
                };
                trials.push(Trial::test(name, move || {
                    run_fixture(style.config()?, &fixture.input, &fixture.expected)
                }));
            }
        }
    }

    if !style_overrides.is_empty() {
        return Err("A fixture with an explicit style override was not discovered".to_string());
    }
    Ok(trials)
}

fn run_fixture(
    cfg: &config::StyleConfig,
    input_path: &Path,
    expected_path: &Path,
) -> Result<(), Failed> {
    let bytes =
        fs::read(input_path).map_err(|e| format!("Cannot read {}: {e}", input_path.display()))?;
    let source = String::from_utf8_lossy(&bytes);
    let expected = fs::read_to_string(expected_path)
        .map_err(|e| format!("Cannot read {}: {e}", expected_path.display()))?;

    let got = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, || formatter::format(&source, cfg))
            .map_err(|e| format!("Cannot spawn formatter thread: {e}"))?
            .join()
            .map_err(|_| "Formatter panicked".to_string())?
            .map_err(|e| format!("Formatter error on {}: {e}", input_path.display()))
    })?;

    if got != expected {
        return Err(diff_message(&input_path.display().to_string(), &expected, &got).into());
    }
    Ok(())
}

fn check_no_unrecognized_options() -> Result<(), Failed> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let xml_dirs = [root.join("tests/fmt"), root.join("tests/fmt/private")];

    let mut xml_paths: Vec<PathBuf> = xml_dirs
        .iter()
        .flat_map(|dir| {
            fs::read_dir(dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("xml"))
        })
        .collect();
    xml_paths.sort();

    if xml_paths.is_empty() {
        return Err("No *.xml files found in tests/fmt/ — check the path".into());
    }

    let mut all_failures: Vec<String> = Vec::new();
    for xml_path in &xml_paths {
        let xml = fs::read_to_string(xml_path)
            .map_err(|e| format!("Cannot read {}: {e}", xml_path.display()))?;
        let unknown = config::collect_unrecognized_options(&xml)
            .map_err(|e| format!("XML parse error in {}: {e}", xml_path.display()))?;
        if !unknown.is_empty() {
            all_failures.push(format!(
                "{}: unrecognized options: {}",
                xml_path.display(),
                unknown.join(", ")
            ));
        }
    }

    if !all_failures.is_empty() {
        return Err(format!(
            "Some style XMLs contain options unknown to ijfmt:\n{}",
            all_failures.join("\n")
        )
        .into());
    }
    Ok(())
}

fn diff_message(label: &str, expected: &str, got: &str) -> String {
    let exp_lines: Vec<&str> = expected.lines().collect();
    let got_lines: Vec<&str> = got.lines().collect();
    let max = exp_lines.len().max(got_lines.len());

    let mut buf = format!(
        "\n=== MISMATCH [{label}] ===\n\
         --- expected\n\
         +++ got\n"
    );

    for i in 0..max {
        let e = exp_lines.get(i).copied();
        let g = got_lines.get(i).copied();
        match (e, g) {
            (Some(el), Some(gl)) if el == gl => buf.push_str(&format!("  {el}\n")),
            (Some(el), Some(gl)) => {
                buf.push_str(&format!("- {el}\n"));
                buf.push_str(&format!("+ {gl}\n"));
            }
            (Some(el), None) => buf.push_str(&format!("- {el}\n")),
            (None, Some(gl)) => buf.push_str(&format!("+ {gl}\n")),
            (None, None) => {}
        }
    }

    buf
}
