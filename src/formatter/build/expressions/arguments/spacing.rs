use super::*;

pub(super) enum ArgumentSpacing {
    Inline {
        space_before_first: bool,
    },
    Multiline {
        first_predecessor: Option<usize>,
        chop_all: bool,
        break_before_last: bool,
        keep_blank_lines: usize,
    },
}

impl ArgumentSpacing {
    pub(super) fn before(&self, source: &ArgumentNodes<'_>, index: usize) -> Spacing {
        let (first_predecessor, chop_all, break_before_last, keep_blank_lines) = match *self {
            Self::Inline { space_before_first } => {
                return if index > 0 || space_before_first {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                };
            }
            Self::Multiline {
                first_predecessor,
                chop_all,
                break_before_last,
                keep_blank_lines,
            } => (
                first_predecessor,
                chop_all,
                break_before_last,
                keep_blank_lines,
            ),
        };
        let arg = source.args[index];
        if index == 0 {
            return first_predecessor.map_or_else(Spacing::none, |row| {
                first_arg_break_spacing(arg, row, keep_blank_lines)
            });
        }
        if !chop_all && break_before_last && index == source.args.len() - 1 {
            return Spacing::new_line();
        }
        let previous = source.args[index - 1];
        if !chop_all && arg.start_position().row <= previous.end_position().row {
            return Spacing::one_space();
        }
        let predecessor_row = source
            .all_children
            .iter()
            .rfind(|node| {
                node.end_byte() <= arg.start_byte() && node.start_byte() >= previous.end_byte()
            })
            .map_or(previous.end_position().row, |node| node.end_position().row);
        let row_gap = arg.start_position().row.saturating_sub(predecessor_row);
        if row_gap >= 2 && keep_blank_lines > 0 {
            Spacing {
                min_spaces: 0,
                max_spaces: 0,
                min_line_feeds: row_gap.min(keep_blank_lines + 1) as u32,
                keep_blank_lines: keep_blank_lines as u32,
                keep_line_breaks: true,
            }
        } else {
            Spacing::new_line()
        }
    }
}

fn first_arg_break_spacing(arg: tree_sitter::Node, predecessor_row: usize, keep: usize) -> Spacing {
    let row_gap = arg.start_position().row.saturating_sub(predecessor_row);
    if arg.kind() == "lambda_expression" && row_gap >= 2 && keep > 0 {
        let capped = row_gap.min(keep + 1);
        Spacing {
            min_spaces: 0,
            max_spaces: 0,
            min_line_feeds: capped as u32,
            keep_blank_lines: keep as u32,
            keep_line_breaks: true,
        }
    } else {
        Spacing::new_line()
    }
}
