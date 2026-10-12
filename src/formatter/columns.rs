pub(super) fn visual_width(text: &str, tab_size: usize) -> usize {
    let tab_size = tab_size.max(1);
    text.chars().fold(0, |column, ch| {
        if ch == '\t' {
            (column / tab_size + 1) * tab_size
        } else {
            column + 1
        }
    })
}

pub(super) fn current_column(out: &str, tab_size: usize) -> usize {
    let line = out.rsplit('\n').next().unwrap_or("");
    visual_width(line, tab_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_advance_to_stops_instead_of_adding_a_fixed_width() {
        assert_eq!(visual_width("ab\tc", 4), 5);
        assert_eq!(visual_width("\t\t", 4), 8);
        assert_eq!(visual_width("a\tb", 0), 3);
        assert_eq!(visual_width("", 4), 0);
    }

    #[test]
    fn text_columns_count_unicode_scalars_not_utf8_bytes() {
        assert_eq!(visual_width("é🙂\tX", 4), 5);
        assert_eq!(visual_width("é🙂", 4), 2);
        assert_eq!(visual_width("e\u{301}", 4), 2);
    }

    #[test]
    fn output_column_only_measures_the_last_line() {
        assert_eq!(current_column("ignored\né\tX", 4), 5);
        assert_eq!(current_column("ignored\r\n", 4), 0);
        assert_eq!(current_column("", 4), 0);
        assert_eq!(visual_width("\n", 4), 1);
    }
}
