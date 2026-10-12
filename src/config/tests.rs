use super::*;

#[test]
fn style_without_import_table_uses_default_layout() {
    let cfg = parse_style_xml("<code_scheme name=\"Default\"/>").unwrap();
    assert_eq!(cfg.import_layout, default_import_layout());
}

#[test]
fn style_parser_loads_import_layout_and_java_settings_together() {
    let xml = r#"<code_scheme name="Custom">
        <JavaCodeStyleSettings>
            <option name="IMPORT_LAYOUT_TABLE"><value>
                <package name="java" withSubpackages="true" static="false"/>
                <emptyLine/>
                <package name="" withSubpackages="true" static="true"/>
            </value></option>
        </JavaCodeStyleSettings>
        <codeStyleSettings language="JAVA">
            <option name="RIGHT_MARGIN" value="72"/>
        </codeStyleSettings>
    </code_scheme>"#;
    let cfg = parse_style_xml(xml).unwrap();
    assert_eq!(cfg.right_margin, 72);
    assert_eq!(
        cfg.import_layout,
        vec![
            ImportGroup::Package {
                prefix: "java".into(),
                is_static: false
            },
            ImportGroup::EmptyLine,
            ImportGroup::Package {
                prefix: String::new(),
                is_static: true
            },
        ]
    );
}

#[test]
fn empty_import_table_preserves_default_layout() {
    let cfg = parse_style_xml(
        r#"<code_scheme><option name="IMPORT_LAYOUT_TABLE"><value/></option></code_scheme>"#,
    )
    .unwrap();
    assert_eq!(cfg.import_layout, default_import_layout());
}

#[test]
fn style_parser_rejects_mismatched_xml_tags() {
    assert!(parse_style_xml("<code_scheme><option></code_scheme>").is_err());
}
