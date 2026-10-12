use crate::config::StyleConfig;
use crate::formatter::build::BlockBuilder;

fn process(raw: &str) -> String {
    let cfg = StyleConfig::default();
    let builder = BlockBuilder::new(raw.as_bytes(), &cfg);
    builder.process_javadoc(raw, 0)
}

#[test]
fn test_p_tag_insertion() {
    let input = "/**\n * First paragraph.\n *\n * Second paragraph.\n */";
    let result = process(input);
    assert!(
        result.contains("* <p>"),
        "Expected <p> insertion, got:\n{}",
        result
    );
    assert!(
        result.contains("* <p>\n * Second paragraph."),
        "Expected <p> before second para"
    );
}

#[test]
fn test_no_p_before_tag() {
    let input = "/**\n * Description.\n *\n * @param foo bar\n */";
    let result = process(input);
    assert!(
        !result.contains("* <p>\n * @param"),
        "Should not insert <p> before @tag"
    );
}

#[test]
fn test_exception_to_throws() {
    let input = "/**\n * @exception IOException if error\n */";
    let result = process(input);
    assert!(
        result.contains("@throws IOException"),
        "Expected @exception -> @throws, got:\n{}",
        result
    );
    assert!(
        !result.contains("@exception"),
        "Should not have @exception anymore"
    );
}

#[test]
fn test_single_line_expansion() {
    let input = "/** Hello world */";
    let result = process(input);
    assert!(
        result.contains("/**\n"),
        "Should expand to multi-line, got:\n{}",
        result
    );
    assert!(
        result.contains("Hello world"),
        "Should contain text, got:\n{}",
        result
    );
}

#[test]
fn test_empty_javadoc_expansion() {
    let input = "/**\n */";
    let result = process(input);
    assert_eq!(
        result.lines().count(),
        3,
        "Should have 3 lines (/**,  *,  */), got:\n{}",
        result
    );
}

#[test]
fn test_blank_line_before_first_tag() {
    let input = "/**\n * Description.\n * @param foo bar\n */";
    let result = process(input);
    let lines: Vec<&str> = result.lines().collect();
    let tag_idx = lines.iter().position(|l| l.contains("@param")).unwrap();
    let prev = lines[tag_idx - 1].trim();
    assert_eq!(
        prev, "*",
        "Should have blank * line before @param, got: '{}'\nFull:\n{}",
        prev, result
    );
}

#[test]
fn test_non_javadoc_unchanged() {
    let input = "/* This is a regular block comment\n * with some content\n */";
    let result = process(input);
    assert_eq!(result, input, "Non-javadoc should be unchanged");
}

#[test]
fn test_no_p_in_pre_block() {
    let input = "/**\n * Example:\n * <pre>\n *\n * code here\n * </pre>\n * More text.\n */";
    let result = process(input);
    let lines: Vec<&str> = result.lines().collect();
    let pre_start = lines.iter().position(|l| l.contains("<pre>")).unwrap();
    let pre_end = lines.iter().position(|l| l.contains("</pre>")).unwrap();
    for (i, line) in lines.iter().enumerate().take(pre_end + 1).skip(pre_start) {
        assert!(
            !line.contains("<p>"),
            "Should not insert <p> inside <pre> at line {}: {}",
            i,
            line
        );
    }
}

#[test]
fn test_align_param_descriptions() {
    let input = "/**\n * @param a the a\n * @param longName the long name\n */";
    let result = process(input);
    let lines: Vec<&str> = result.lines().collect();
    let param_a = lines.iter().find(|l| l.contains("@param a")).unwrap();
    let param_long = lines
        .iter()
        .find(|l| l.contains("@param longName"))
        .unwrap();
    let desc_col_a = param_a.find("the a").unwrap();
    let desc_col_long = param_long.find("the long").unwrap();
    assert_eq!(
        desc_col_a, desc_col_long,
        "Descriptions should be aligned.\nLine 1: {}\nLine 2: {}",
        param_a, param_long
    );
}

#[test]
fn test_align_throws_descriptions() {
    let input = "/**\n * @throws IOException if I/O error\n * @throws IllegalStateException if bad state\n */";
    let result = process(input);
    let lines: Vec<&str> = result.lines().collect();
    let throws_io = lines
        .iter()
        .find(|l| l.contains("@throws IOException"))
        .unwrap();
    let throws_ise = lines
        .iter()
        .find(|l| l.contains("@throws IllegalStateException"))
        .unwrap();
    let desc_col_io = throws_io.find("if I/O").unwrap();
    let desc_col_ise = throws_ise.find("if bad").unwrap();
    assert_eq!(
        desc_col_io, desc_col_ise,
        "Descriptions should be aligned.\nLine 1: {}\nLine 2: {}",
        throws_io, throws_ise
    );
}

#[test]
fn test_no_align_single_param() {
    let input = "/**\n * @param foo the foo\n */";
    let result = process(input);
    assert!(
        result.contains("@param foo the foo"),
        "Single param should not be padded, got:\n{}",
        result
    );
}

#[test]
fn test_align_param_disabled() {
    let input = "/**\n * @param a the a\n * @param longName the long name\n */";
    let cfg = StyleConfig {
        jd_align_param_comments: false,
        ..StyleConfig::default()
    };
    let builder = BlockBuilder::new(input.as_bytes(), &cfg);
    let result = builder.process_javadoc(input, 0);
    assert!(
        result.contains("@param a the a"),
        "With alignment disabled, should not pad, got:\n{}",
        result
    );
}
