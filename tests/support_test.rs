mod support;

use std::fs;
use support::TestDir;

#[test]
fn test_directories_are_unique_and_cleaned_up_independently() {
    let first = TestDir::new();
    let second = TestDir::new();
    assert_ne!(first.path(), second.path());
    let first_path = first.path().to_path_buf();
    fs::write(first.path().join("first"), "one").unwrap();
    fs::write(second.path().join("second"), "two").unwrap();
    drop(first);
    assert!(!first_path.exists());
    assert_eq!(
        fs::read_to_string(second.path().join("second")).unwrap(),
        "two"
    );
}
