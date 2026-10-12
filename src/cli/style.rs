use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};

fn find_idea_config(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        let candidate = dir.join(".idea/codeStyles/codeStyleConfig.xml");
        if candidate.exists() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

fn parse_style_name(config_xml: &Path) -> Option<String> {
    let content = fs::read_to_string(config_xml).ok()?;
    let after = content.split("PREFERRED_PROJECT_CODE_STYLE").nth(1)?;
    let value_start = after.find("value=\"")? + 7;
    let value_end = after[value_start..].find('"')?;
    Some(after[value_start..value_start + value_end].to_string())
}

fn find_jetbrains_codestyles_dir() -> Option<PathBuf> {
    let base = dirs::home_dir()?.join(".config/JetBrains");
    let mut best: Option<(String, String)> = None;
    for entry in fs::read_dir(&base).ok()?.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => continue,
        };
        let is_idea = name.starts_with("IdeaIC") || name.starts_with("IntelliJIdea");
        if !is_idea || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let version = name
            .trim_start_matches(|character: char| !character.is_ascii_digit())
            .to_string();
        if best.as_ref().is_none_or(|(current, _)| version > *current) {
            best = Some((version, name));
        }
    }
    Some(base.join(best?.1).join("codestyles"))
}

fn ijfmt_config_dir() -> Option<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(xdg) if !xdg.is_empty() => Some(PathBuf::from(xdg).join("ijfmt")),
        _ => Some(dirs::home_dir()?.join(".config/ijfmt")),
    }
}

fn find_style_by_name(name: &str) -> Option<PathBuf> {
    let file = format!("{name}.xml");
    [
        ijfmt_config_dir().map(|dir| dir.join("styles")),
        find_jetbrains_codestyles_dir(),
    ]
    .into_iter()
    .flatten()
    .map(|dir| dir.join(&file))
    .find(|path| path.exists())
}

fn global_default_style() -> Option<String> {
    let content = fs::read_to_string(ijfmt_config_dir()?.join("config")).ok()?;
    for line in content.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "style" {
            continue;
        }
        let value = value.trim().trim_matches('"');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn parse_code_scheme_name(xml_path: &Path) -> Option<String> {
    let content = fs::read_to_string(xml_path).ok()?;
    let after = content.split("code_scheme").nth(1)?;
    let name_start = after.find("name=\"")? + 6;
    let name_end = after[name_start..].find('"')?;
    Some(after[name_start..name_start + name_end].to_string())
}

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        if [".git", ".idea"]
            .iter()
            .any(|marker| dir.join(marker).exists())
        {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

fn set_project_default(search_root: &Path, fallback_path: &Path, style_name: &str) {
    let project_root = match find_project_root(search_root) {
        Some(root) => root,
        None => {
            eprintln!("warning: could not find project root (no .git or .idea found)");
            return;
        }
    };

    let idea_dir = project_root.join(".idea/codeStyles");
    if fs::create_dir_all(&idea_dir).is_ok() {
        let config_content = format!(
            "<component name=\"ProjectCodeStyleConfiguration\">\n  \
             <option name=\"PREFERRED_PROJECT_CODE_STYLE\" value=\"{style_name}\" />\n\
             </component>\n"
        );
        if let Err(error) = fs::write(idea_dir.join("codeStyleConfig.xml"), config_content) {
            eprintln!("warning: could not write codeStyleConfig.xml: {error}");
        } else {
            eprintln!("info: wrote .idea/codeStyles/codeStyleConfig.xml (style: {style_name})");
        }
    }

    if let Some(codestyles_dir) = find_jetbrains_codestyles_dir() {
        let destination = codestyles_dir.join(format!("{style_name}.xml"));
        if !destination.exists() {
            if let Err(error) = fs::copy(fallback_path, &destination) {
                eprintln!(
                    "warning: could not copy style to {}: {error}",
                    destination.display()
                );
            } else {
                eprintln!("info: copied style to {}", destination.display());
            }
        }
    }
}

pub(crate) fn resolve_style(
    explicit: Option<PathBuf>,
    fallback: Option<PathBuf>,
    search_root: &Path,
    persist_fallback: bool,
) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }

    let configured_style = find_idea_config(search_root)
        .as_deref()
        .and_then(parse_style_name);

    if let Some(style_name) = &configured_style {
        if let Some(path) = find_style_by_name(style_name) {
            return Ok(path);
        }
        eprintln!(
            "warning: project style {style_name:?} not found in ~/.config/ijfmt/styles or the \
             JetBrains codestyles directory"
        );
    }

    if let Some(name) = global_default_style() {
        if let Some(path) = find_style_by_name(&name) {
            return Ok(path);
        }
        eprintln!(
            "warning: global default style {name:?} (from ~/.config/ijfmt/config) not found in \
             ~/.config/ijfmt/styles or the JetBrains codestyles directory"
        );
    }

    if let Some(fallback) = fallback {
        if !fallback.exists() {
            return Err(anyhow!("fallback style not found: {}", fallback.display()));
        }
        if persist_fallback
            && configured_style.is_none()
            && let Some(name) = parse_code_scheme_name(&fallback)
        {
            set_project_default(search_root, &fallback, &name);
        }
        return Ok(fallback);
    }

    let executable_dir = std::env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow!("could not determine binary directory"))?
        .to_path_buf();

    for candidate in [
        executable_dir.join("Sample.xml"),
        executable_dir.join("../tests/fmt/Sample.xml"),
    ] {
        let candidate = candidate.canonicalize().unwrap_or(candidate);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "no style XML found: provide --style or --fallback, set `style = <name>` in \
         ~/.config/ijfmt/config, or add .idea/codeStyles/codeStyleConfig.xml to your project"
    ))
}
