use super::options::{OptionSection, apply_option};
use super::{StyleConfig, parse_import_layout};

use quick_xml::Reader;
use quick_xml::events::Event;

pub fn parse_style_xml(xml: &str) -> anyhow::Result<StyleConfig> {
    let mut cfg = StyleConfig::default();
    walk_style_xml(xml, |opt_name, opt_value, section| {
        apply_option(&mut cfg, opt_name, opt_value, section);
    })?;
    if let Some(layout) = parse_import_layout(xml) {
        cfg.import_layout = layout;
    }
    Ok(cfg)
}

pub fn collect_unrecognized_options(xml: &str) -> anyhow::Result<Vec<String>> {
    let mut unknown: Vec<String> = Vec::new();
    walk_style_xml(xml, |opt_name, opt_value, section| {
        if !apply_option(&mut StyleConfig::default(), opt_name, opt_value, section) {
            unknown.push(opt_name.to_owned());
        }
    })?;
    unknown.dedup();
    Ok(unknown)
}

fn walk_style_xml(
    xml: &str,
    mut on_option: impl FnMut(&str, &str, OptionSection),
) -> anyhow::Result<()> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut in_java_code_style = false;
    let mut in_java_global = false;
    let mut in_indent_options = false;
    let mut in_other_indent_options = false;
    let mut in_non_java_code_style = false;
    let mut in_non_java_settings = false;
    let mut depth_java: i32 = 0;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let raw_name = e.name();
                let name = std::str::from_utf8(raw_name.as_ref())
                    .unwrap_or("")
                    .to_owned();

                match name.as_str() {
                    "codeStyleSettings" => {
                        let mut is_java = false;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"language" {
                                let val = std::str::from_utf8(attr.value.as_ref()).unwrap_or("");
                                if val == "JAVA" {
                                    is_java = true;
                                    in_java_code_style = true;
                                    depth_java = 1;
                                }
                            }
                        }
                        if !is_java {
                            in_non_java_code_style = true;
                        }
                    }
                    "JavaCodeStyleSettings" => {
                        in_java_global = true;
                    }
                    "XML" => {
                        in_non_java_settings = true;
                    }
                    "indentOptions" if in_java_code_style => {
                        in_indent_options = true;
                    }
                    "option" if !in_non_java_code_style && !in_non_java_settings => {
                        let mut opt_name = String::new();
                        let mut opt_value = String::new();
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"name" => {
                                    opt_name = std::str::from_utf8(attr.value.as_ref())
                                        .unwrap_or("")
                                        .to_owned()
                                }
                                b"value" => {
                                    opt_value = std::str::from_utf8(attr.value.as_ref())
                                        .unwrap_or("")
                                        .to_owned()
                                }
                                _ => {}
                            }
                        }
                        if opt_name == "OTHER_INDENT_OPTIONS"
                            && opt_value.is_empty()
                            && !in_java_code_style
                            && !in_java_global
                        {
                            in_other_indent_options = true;
                        } else {
                            let section = if in_java_global {
                                OptionSection::JavaGlobal
                            } else if !in_java_code_style {
                                OptionSection::Scheme
                            } else if in_indent_options || in_other_indent_options {
                                OptionSection::JavaIndentation
                            } else {
                                OptionSection::Java
                            };
                            on_option(&opt_name, &opt_value, section);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let raw_name = e.name();
                let name = std::str::from_utf8(raw_name.as_ref()).unwrap_or("");
                match name {
                    "codeStyleSettings" if in_java_code_style => {
                        depth_java -= 1;
                        if depth_java <= 0 {
                            in_java_code_style = false;
                            in_indent_options = false;
                        }
                    }
                    "codeStyleSettings" if in_non_java_code_style => {
                        in_non_java_code_style = false;
                    }
                    "JavaCodeStyleSettings" => {
                        in_java_global = false;
                    }
                    "XML" if in_non_java_settings => {
                        in_non_java_settings = false;
                    }
                    "indentOptions" => {
                        in_indent_options = false;
                    }
                    "value" if in_other_indent_options => {
                        in_other_indent_options = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("XML parse error: {e}")),
            _ => {}
        }
    }

    Ok(())
}
