use quick_xml::Reader;
use quick_xml::events::Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportGroup {
    EmptyLine,
    Package { prefix: String, is_static: bool },
    GroupByTopLevel,
}

pub fn default_import_layout() -> Vec<ImportGroup> {
    vec![ImportGroup::GroupByTopLevel]
}

pub fn parse_import_layout(xml: &str) -> Option<Vec<ImportGroup>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut in_import_layout = false;
    let mut entries: Vec<ImportGroup> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let raw_name = e.name();
                let name = std::str::from_utf8(raw_name.as_ref())
                    .unwrap_or("")
                    .to_owned();

                if name == "option" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"name"
                            && std::str::from_utf8(attr.value.as_ref()).unwrap_or("")
                                == "IMPORT_LAYOUT_TABLE"
                        {
                            in_import_layout = true;
                            break;
                        }
                    }
                }

                if in_import_layout {
                    if name == "package" {
                        let mut prefix = String::new();
                        let mut is_static = false;
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"name" => {
                                    prefix = std::str::from_utf8(attr.value.as_ref())
                                        .unwrap_or("")
                                        .to_owned();
                                }
                                b"static" => {
                                    is_static = std::str::from_utf8(attr.value.as_ref())
                                        .unwrap_or("")
                                        == "true";
                                }
                                _ => {}
                            }
                        }
                        entries.push(ImportGroup::Package { prefix, is_static });
                    } else if name == "emptyLine" {
                        entries.push(ImportGroup::EmptyLine);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let raw_name = e.name();
                if std::str::from_utf8(raw_name.as_ref()).unwrap_or("") == "option"
                    && in_import_layout
                {
                    in_import_layout = false;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}
