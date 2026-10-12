use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineSelection {
    All,
    Ranges(Vec<Range<usize>>),
}

impl LineSelection {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Ranges(ranges) if ranges.is_empty())
    }

    fn overlaps(&self, lines: Range<usize>) -> bool {
        match self {
            Self::All => true,
            Self::Ranges(ranges) => ranges
                .iter()
                .any(|range| range.start < lines.end && lines.start < range.end),
        }
    }
}

pub(super) fn apply_to_selected(
    source: &str,
    formatted: &str,
    selection: &LineSelection,
) -> String {
    let old: Vec<&str> = source.split_inclusive('\n').collect();
    let new: Vec<&str> = formatted.split_inclusive('\n').collect();
    let old_keys: Vec<String> = old.iter().map(|line| key(line)).collect();
    let new_keys: Vec<String> = new.iter().map(|line| key(line)).collect();

    let mut output = String::with_capacity(source.len());
    for op in capture_diff_slices(Algorithm::Myers, &old_keys, &new_keys) {
        let (old_range, new_range) = (op.old_range(), op.new_range());
        if let DiffOp::Equal { .. } = op {
            for (old_index, new_index) in old_range.zip(new_range) {
                let take_new = selection.overlaps(old_index..old_index + 1);
                output.push_str(if take_new {
                    new[new_index]
                } else {
                    old[old_index]
                });
            }
            continue;
        }
        let neighbourhood = if old_range.is_empty() {
            old_range.start.saturating_sub(1)..old_range.end + 1
        } else {
            old_range.clone()
        };
        let lines = if selection.overlaps(neighbourhood) {
            &new[new_range]
        } else {
            &old[old_range]
        };
        lines.iter().for_each(|line| output.push_str(line));
    }
    output
}

fn key(line: &str) -> String {
    line.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{LineSelection, apply_to_selected};
    use std::ops::Range;

    fn only(range: Range<usize>) -> LineSelection {
        LineSelection::Ranges(vec![range])
    }

    const SOURCE: &str = "class A {\n  int a;\nint b;\n    int c;\n}\n";
    const FORMATTED: &str = "class A {\n    int a;\n    int b;\n    int c;\n}\n";

    #[test]
    fn only_selected_lines_are_reindented() {
        let selection = only(2..3);
        assert_eq!(
            apply_to_selected(SOURCE, FORMATTED, &selection),
            "class A {\n  int a;\n    int b;\n    int c;\n}\n"
        );
    }

    #[test]
    fn an_empty_selection_keeps_the_source() {
        assert_eq!(
            apply_to_selected(SOURCE, FORMATTED, &LineSelection::Ranges(vec![])),
            SOURCE
        );
    }

    #[test]
    fn selecting_all_takes_the_formatted_text() {
        assert_eq!(
            apply_to_selected(SOURCE, FORMATTED, &LineSelection::All),
            FORMATTED
        );
    }

    #[test]
    fn joined_lines_are_taken_together() {
        let source = "class A {\n  void f(int a,\n      int b) {}\n  int  x;\n}\n";
        let formatted = "class A {\n    void f(int a, int b) {}\n\n    int x;\n}\n";
        assert_eq!(
            apply_to_selected(source, formatted, &only(2..3)),
            "class A {\n    void f(int a, int b) {}\n\n  int  x;\n}\n"
        );
    }

    #[test]
    fn missing_final_newline_is_kept_when_unselected() {
        let selection = only(0..1);
        assert_eq!(apply_to_selected("a  \nb", "a\nb\n", &selection), "a\nb");
    }
}
