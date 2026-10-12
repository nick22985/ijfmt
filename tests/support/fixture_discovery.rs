use std::fs;
use std::path::{Path, PathBuf};

pub struct Fixture {
    pub name: String,
    pub input: PathBuf,
    pub expected: PathBuf,
}

pub struct StyleFixtures {
    pub name: String,
    pub xml: PathBuf,
    pub fixtures: Vec<Fixture>,
}

pub fn discover_optional(styles: &Path, corpus: &Path) -> Result<Vec<StyleFixtures>, String> {
    for root in [styles, corpus] {
        if !root
            .try_exists()
            .map_err(|error| format!("Cannot inspect {}: {error}", root.display()))?
        {
            return Ok(Vec::new());
        }
    }
    discover(styles, corpus)
}

pub fn discover(styles: &Path, corpus: &Path) -> Result<Vec<StyleFixtures>, String> {
    let mut found = Vec::new();
    for entry in entries(styles)? {
        let xml = entry.path();
        if xml.extension().and_then(|s| s.to_str()) != Some("xml") {
            continue;
        }
        let name = xml
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("Invalid style name: {}", xml.display()))?;
        let name = name.to_lowercase();
        let directory = corpus.join(&name);
        if !directory
            .try_exists()
            .map_err(|error| format!("Cannot inspect {}: {error}", directory.display()))?
        {
            continue;
        }
        let mut fixtures = Vec::new();
        collect(&directory, &directory, &mut fixtures)?;
        if fixtures.is_empty() {
            return Err(format!("No Java fixture pairs in {}", directory.display()));
        }
        fixtures.sort_by(|a, b| a.name.cmp(&b.name));
        found.push(StyleFixtures {
            name,
            xml,
            fixtures,
        });
    }
    if found.is_empty() {
        return Err(format!(
            "No Java fixture pairs for styles in {} beneath {}",
            styles.display(),
            corpus.display()
        ));
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

fn entries(directory: &Path) -> Result<Vec<fs::DirEntry>, String> {
    fs::read_dir(directory)
        .and_then(|entries| entries.collect())
        .map_err(|error| format!("Cannot list {}: {error}", directory.display()))
}

fn collect(directory: &Path, root: &Path, fixtures: &mut Vec<Fixture>) -> Result<(), String> {
    for entry in entries(directory)? {
        let input = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| format!("Cannot inspect {}: {error}", input.display()))?;
        if kind.is_dir() {
            collect(&input, root, fixtures)?;
        } else if kind.is_file() {
            let relative = input
                .strip_prefix(root)
                .map_err(|error| error.to_string())?;
            let Some(stem) = relative
                .to_str()
                .and_then(|s| s.strip_suffix(".input.java"))
            else {
                continue;
            };
            let expected = root.join(format!("{stem}.expected.java"));
            if !expected
                .try_exists()
                .map_err(|error| format!("Cannot inspect {}: {error}", expected.display()))?
            {
                return Err(format!("Missing expected output: {}", expected.display()));
            }
            fixtures.push(Fixture {
                name: stem.replace(std::path::MAIN_SEPARATOR, "/"),
                input,
                expected,
            });
        }
    }
    Ok(())
}
