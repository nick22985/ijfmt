use super::*;

impl<'a> BlockBuilder<'a> {
    pub(super) fn control_child_spacing(
        &self,
        context: &ControlStatement<'_, '_>,
        child: Node,
        previous: &PreviousControlChild,
        placement: ControlBodyPlacement,
    ) -> (Spacing, bool) {
        let node = context.node;
        let body_indices = context.body_indices;
        let ControlBodyPlacement {
            is_nonblock_body,
            is_else_if,
            needs_braces,
            keep_one_line,
        } = placement;
        let else_if_glued = self.cfg.special_else_if_treatment && is_else_if;

        let config_spacing = match child.kind() {
            "catch_clause" => Some(self.cfg.catch_on_new_line),
            "finally_clause" => Some(self.cfg.finally_on_new_line),
            "else" => {
                if previous.bound_kind.as_deref() == Some("block") {
                    Some(self.cfg.else_on_new_line)
                } else {
                    Some(true)
                }
            }
            _ => None,
        };

        let is_body_block = (child.kind() == "block" || child.kind() == "switch_block")
            && body_indices.contains(&child.id());

        let is_paren_child = child.kind() == "parenthesized_expression"
            || child.kind() == "("
            || child.kind() == "resource_specification";
        let keyword_paren_space = if is_paren_child {
            match previous.kind.as_deref() {
                Some("if") => Some(self.cfg.space_before_if_parentheses),
                Some("while") => Some(self.cfg.space_before_while_parentheses),
                Some("for") => Some(self.cfg.space_before_for_parentheses),
                Some("switch") => Some(self.cfg.space_before_switch_parentheses),
                Some("synchronized") => Some(self.cfg.space_before_synchronized_parentheses),
                Some("try") => Some(true),
                _ => None,
            }
        } else {
            None
        };

        let foreach_colon_space = if node.kind() == "enhanced_for_statement" {
            if child.kind() == ":" {
                Some(self.cfg.space_before_colon_in_foreach)
            } else if previous.kind.as_deref() == Some(":") {
                Some(self.cfg.space_after_colon_in_foreach)
            } else {
                None
            }
        } else {
            None
        };

        let spacing = if let Some(pb) = previous.end_byte {
            if else_if_glued {
                Spacing::one_space()
            } else if let Some(on_new_line) = config_spacing {
                let row_gap = child
                    .start_position()
                    .row
                    .saturating_sub(previous.end_row.unwrap_or(0));
                if on_new_line {
                    if row_gap >= 2 && self.cfg.keep_blank_lines_in_code > 0 {
                        let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1);
                        Spacing {
                            min_spaces: 0,
                            max_spaces: 0,
                            min_line_feeds: capped as u32,
                            keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                            keep_line_breaks: true,
                        }
                    } else {
                        Spacing::new_line()
                    }
                } else {
                    Spacing::one_space()
                }
            } else if let Some(want_space) = keyword_paren_space {
                if want_space {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                }
            } else if let Some(want_space) = foreach_colon_space {
                let n = if want_space { 1 } else { 0 };
                Spacing {
                    min_spaces: n,
                    max_spaces: n,
                    min_line_feeds: 0,
                    keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                    keep_line_breaks: true,
                }
            } else if child.is_extra() && previous.end_row == Some(child.start_position().row) {
                let gap = &self.src[pb..child.start_byte()];
                let ts = self.cfg.indent_size.max(1) as u32;
                let sp = gap
                    .iter()
                    .map(|&b| {
                        if b == b'\t' {
                            ts
                        } else if b == b' ' {
                            1
                        } else {
                            0
                        }
                    })
                    .sum::<u32>();
                Spacing::spaces(sp, sp)
            } else if is_body_block && self.cfg.brace_style == BraceStyle::NextLine {
                self.lbrace_new_line(child)
            } else if is_body_block && self.cfg.brace_style == BraceStyle::EndOfLine {
                if previous.kind.as_deref() == Some("line_comment") {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                }
            } else if needs_braces {
                if self.cfg.brace_style == BraceStyle::NextLine
                    || previous.kind.as_deref() == Some("line_comment")
                {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                }
            } else if node.kind() == "for_statement"
                && child.kind() == ";"
                && matches!(
                    previous.kind.as_deref(),
                    Some(";" | "local_variable_declaration")
                )
            {
                Spacing::one_space()
            } else if node.kind() == "for_statement"
                && previous.kind.as_deref() == Some(";")
                && child.kind() == ")"
            {
                if previous
                    .end_row
                    .is_some_and(|pr| child.start_position().row > pr)
                {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                }
            } else if node.kind() == "labeled_statement"
                && previous.kind.as_deref() == Some(":")
                && !child.is_extra()
            {
                let kept_inline_body = self.cfg.keep_control_statement_in_one_line
                    && node.parent().is_some_and(|p| {
                        matches!(
                            p.kind(),
                            "if_statement"
                                | "for_statement"
                                | "enhanced_for_statement"
                                | "while_statement"
                                | "do_statement"
                        ) && p.start_position().row == node.start_position().row
                    });
                if kept_inline_body {
                    Spacing::none()
                } else {
                    Spacing::new_line()
                }
            } else if is_nonblock_body
                && !keep_one_line
                && !is_else_if
                && child.start_position().row == node.start_position().row
            {
                Spacing::new_line()
            } else if is_nonblock_body && previous.end_row == Some(child.start_position().row) {
                Spacing::one_space()
            } else if child.start_position().row > previous.end_row.unwrap_or(0) {
                let row_gap2 = child
                    .start_position()
                    .row
                    .saturating_sub(previous.end_row.unwrap_or(0));
                if row_gap2 >= 2 && self.cfg.keep_blank_lines_in_code > 0 {
                    let capped = row_gap2.min(self.cfg.keep_blank_lines_in_code + 1);
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: capped as u32,
                        keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                        keep_line_breaks: true,
                    }
                } else {
                    Spacing::new_line()
                }
            } else if node.kind() == "for_statement"
                && matches!(
                    previous.kind.as_deref(),
                    Some(";" | "local_variable_declaration")
                )
            {
                Spacing::one_space()
            } else if matches!(node.kind(), "for_statement" | "enhanced_for_statement")
                && (previous.kind.as_deref() == Some("(") || matches!(child.kind(), ")" | ";"))
            {
                Spacing::none()
            } else if child.start_byte() > pb {
                let gap = &self.src[pb..child.start_byte()];
                if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                }
            } else {
                Spacing::none()
            }
        } else {
            Spacing::none()
        };
        (spacing, foreach_colon_space.is_some())
    }
}
