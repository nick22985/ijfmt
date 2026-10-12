#[path = "support/fixture_discovery.rs"]
mod fixture_discovery;
mod support;

use std::fs;
use support::TestDir;

fn style(dir: &TestDir) {
    fs::create_dir(dir.path().join("styles")).unwrap();
    fs::write(dir.path().join("styles/Sample.xml"), "<code_scheme/>").unwrap();
}

#[test]
fn missing_styles_are_an_error() {
    let dir = TestDir::new();
    assert!(
        fixture_discovery::discover(&dir.path().join("styles"), &dir.path().join("corpus"))
            .is_err()
    );
}

#[test]
fn private_corpus_remains_optional_but_public_coverage_is_required() {
    let dir = TestDir::new();
    style(&dir);
    let styles = dir.path().join("styles");
    let corpus = dir.path().join("corpus");
    assert!(
        fixture_discovery::discover_optional(&styles, &corpus)
            .unwrap()
            .is_empty()
    );
    assert!(fixture_discovery::discover(&styles, &corpus).is_err());
    fs::create_dir(&corpus).unwrap();
    assert!(fixture_discovery::discover_optional(&styles, &corpus).is_err());
}

#[test]
fn absent_or_empty_corpus_cannot_report_success() {
    let dir = TestDir::new();
    style(&dir);
    assert!(
        fixture_discovery::discover(&dir.path().join("styles"), &dir.path().join("corpus"))
            .is_err()
    );
    fs::create_dir_all(dir.path().join("corpus/sample")).unwrap();
    assert!(
        fixture_discovery::discover(&dir.path().join("styles"), &dir.path().join("corpus"))
            .is_err()
    );
}

#[test]
fn nested_cases_are_found_with_distinct_names() {
    let dir = TestDir::new();
    style(&dir);
    for name in ["Foo", "nested/Foo"] {
        let base = dir.path().join("corpus/sample").join(name);
        fs::create_dir_all(base.parent().unwrap()).unwrap();
        for suffix in ["input.java", "expected.java"] {
            fs::write(base.with_extension(suffix), "class Foo {}\n").unwrap();
        }
    }
    let styles =
        fixture_discovery::discover(&dir.path().join("styles"), &dir.path().join("corpus"))
            .unwrap();
    assert_eq!(styles.len(), 1);
    assert_eq!(styles[0].name, "sample");
    assert_eq!(styles[0].xml, dir.path().join("styles/Sample.xml"));
    assert_eq!(
        styles[0]
            .fixtures
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["Foo", "nested/Foo"]
    );
    assert!(
        styles[0]
            .fixtures
            .iter()
            .all(|f| f.input.is_file() && f.expected.is_file())
    );
}

#[test]
fn missing_expected_output_is_an_error() {
    let dir = TestDir::new();
    style(&dir);
    fs::create_dir_all(dir.path().join("corpus/sample")).unwrap();
    fs::write(
        dir.path().join("corpus/sample/Foo.input.java"),
        "class Foo {}\n",
    )
    .unwrap();
    assert!(
        fixture_discovery::discover(&dir.path().join("styles"), &dir.path().join("corpus"))
            .is_err()
    );
}
