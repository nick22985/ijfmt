use super::*;

pub(super) enum ArgumentPlacement {
    Wrapped,
    AfterOwnLineComma,
}

pub(super) struct LeadingComments {
    pub(super) predecessor_row: usize,
    pub(super) inline_space: bool,
}

impl BlockBuilder<'_> {
    pub(super) fn leading_argument_comments(
        &mut self,
        source: &ArgumentNodes<'_>,
        lparen_row: usize,
        arg_indent: IndentType,
        children: &mut Vec<Block>,
    ) -> LeadingComments {
        let ArgumentNodes {
            node,
            all_children,
            args,
            comments,
        } = source;
        let lparen_end_byte = all_children
            .iter()
            .find(|n| n.kind() == "(")
            .map(|n| n.end_byte())
            .unwrap_or(node.start_byte());
        let mut last_inline_comment_end: Option<usize> = None;
        let mut first_arg_predecessor_row = lparen_row;
        for c in comments {
            if c.start_byte() >= lparen_end_byte && c.start_byte() < args[0].start_byte() {
                let on_own_line = c.start_position().row > lparen_row;
                let is_line_comment = c.kind() == "line_comment";
                let spacing = if on_own_line {
                    Spacing::new_line()
                } else if is_line_comment {
                    let gap_start = lparen_end_byte;
                    let gap_end = c.start_byte().min(self.src.len());
                    let spaces = if gap_start < gap_end {
                        self.src[gap_start..gap_end]
                            .iter()
                            .filter(|&&b| b == b' ' || b == b'\t')
                            .count()
                            .max(1) as u32
                    } else {
                        1
                    };
                    Spacing::spaces(spaces, spaces)
                } else {
                    Spacing::none()
                };
                let block = self.comment_leaf(*c).with_spacing(spacing);
                let block = if on_own_line {
                    block.with_indent(arg_indent).with_arg_probe()
                } else {
                    last_inline_comment_end = Some(c.end_byte());
                    block
                };
                children.push(block);
                first_arg_predecessor_row = first_arg_predecessor_row.max(c.end_position().row);
            }
        }
        let had_inline_comment_before_first = last_inline_comment_end.is_some_and(|end| {
            let gap = &self.src[end..args[0].start_byte()];
            gap.iter().any(|&b| b == b' ' || b == b'\t')
        });

        LeadingComments {
            predecessor_row: first_arg_predecessor_row,
            inline_space: had_inline_comment_before_first,
        }
    }
    pub(super) fn trailing_argument_comments(
        &mut self,
        source: &ArgumentNodes<'_>,
        arg_indent: IndentType,
        children: &mut Vec<Block>,
    ) -> usize {
        let ArgumentNodes {
            node,
            all_children,
            args,
            comments,
        } = source;
        let mut last_content_row = args.last().map(|a| a.end_position().row).unwrap_or(0);
        if let Some(last_arg) = args.last() {
            let rparen_start_byte = all_children
                .iter()
                .find(|n| n.kind() == ")")
                .map(|n| n.start_byte())
                .unwrap_or(node.end_byte());
            for c in comments {
                if c.start_byte() >= last_arg.end_byte() && c.start_byte() < rparen_start_byte {
                    let on_own_line = c.start_position().row > last_arg.end_position().row;
                    let spacing = if on_own_line {
                        Spacing::new_line()
                    } else {
                        let gap_start = last_arg.end_byte().min(c.start_byte());
                        let gap_end = c.start_byte().min(self.src.len());
                        let spaces = if gap_start < gap_end {
                            self.src[gap_start..gap_end]
                                .iter()
                                .filter(|&&b| b == b' ' || b == b'\t')
                                .count() as u32
                        } else {
                            0
                        };
                        if spaces > 0 {
                            Spacing::spaces(spaces, spaces)
                        } else {
                            Spacing::none()
                        }
                    };
                    let comment_block = self.comment_leaf(*c).with_spacing(spacing);
                    let comment_block = if on_own_line {
                        comment_block
                            .with_indent(arg_indent)
                            .with_arg_probe()
                            .with_anchor_exempt()
                    } else {
                        comment_block
                    };
                    children.push(comment_block);
                    last_content_row = last_content_row.max(c.end_position().row);
                }
            }
        }

        last_content_row
    }
    pub(super) fn argument_separator(
        &mut self,
        source: &ArgumentNodes<'_>,
        i: usize,
        source_multiline: bool,
        arg_indent: IndentType,
        children: &mut Vec<Block>,
    ) -> ArgumentPlacement {
        let ArgumentNodes {
            all_children,
            args,
            comments,
            ..
        } = source;
        let arg = args[i];
        let prev_arg = args[i - 1];
        let comma_node = all_children.iter().find(|n| {
            n.kind() == ","
                && n.start_byte() > prev_arg.end_byte()
                && n.start_byte() < arg.start_byte()
        });
        let comma_byte = comma_node
            .map(|n| n.start_byte())
            .unwrap_or(prev_arg.end_byte());

        let comma_on_new_line = source_multiline
            && comma_node
                .map(|n| n.start_position().row > prev_arg.end_position().row)
                .unwrap_or(false);

        if comma_on_new_line {
            for c in comments {
                if c.start_byte() >= prev_arg.end_byte() && c.start_byte() < comma_byte {
                    let on_own_line = c.start_position().row > prev_arg.end_position().row;
                    let spacing = if on_own_line {
                        Spacing::new_line()
                    } else {
                        Spacing::one_space()
                    };
                    let block = self.comment_leaf(*c).with_spacing(spacing);
                    let block = if on_own_line {
                        block.with_indent(arg_indent)
                    } else {
                        block
                    };
                    children.push(block);
                }
            }

            children.push(
                Block::leaf(",")
                    .with_spacing(Spacing::new_line())
                    .with_indent(arg_indent)
                    .with_arg_probe(),
            );

            let comma_end = comma_node.map(|n| n.end_byte()).unwrap_or(comma_byte + 1);
            for c in comments {
                if c.start_byte() >= comma_end && c.start_byte() < arg.start_byte() {
                    let on_own_line = c.start_position().row
                        > comma_node
                            .map(|n| n.end_position().row)
                            .unwrap_or(prev_arg.end_position().row);
                    let spacing = if on_own_line {
                        Spacing::new_line()
                    } else {
                        Spacing::one_space()
                    };
                    let block = self.comment_leaf(*c).with_spacing(spacing);
                    let block = if on_own_line {
                        block.with_indent(arg_indent)
                    } else {
                        block
                    };
                    children.push(block);
                }
            }

            ArgumentPlacement::AfterOwnLineComma
        } else {
            for c in comments {
                if c.start_byte() >= prev_arg.end_byte() && c.start_byte() < comma_byte {
                    let on_own_line = c.start_position().row > prev_arg.end_position().row;
                    let spacing = if on_own_line {
                        Spacing::new_line()
                    } else {
                        let gap_start = prev_arg.end_byte();
                        let gap_end = c.start_byte().min(self.src.len());
                        let spaces = if gap_start < gap_end {
                            self.src[gap_start..gap_end]
                                .iter()
                                .filter(|&&b| b == b' ' || b == b'\t')
                                .count() as u32
                        } else {
                            0
                        };
                        if spaces > 0 {
                            Spacing::spaces(spaces, spaces)
                        } else {
                            Spacing::none()
                        }
                    };
                    let comment_block = self.comment_leaf(*c).with_spacing(spacing);
                    let comment_block = if on_own_line {
                        comment_block.with_indent(arg_indent)
                    } else {
                        comment_block
                    };
                    children.push(comment_block);
                }
            }

            children.push(Block::leaf(",").with_spacing(Spacing::none()));

            let comma_end_byte = comma_node.map(|n| n.end_byte()).unwrap_or(comma_byte + 1);
            let comma_row = comma_node
                .map(|n| n.end_position().row)
                .unwrap_or(prev_arg.end_position().row);
            let mut between_prev_row = comma_row;
            for c in comments {
                if c.start_byte() > comma_byte && c.start_byte() < arg.start_byte() {
                    let on_own_line = c.start_position().row > prev_arg.end_position().row;
                    let spacing = if on_own_line {
                        let c_row = c.start_position().row;
                        let row_gap = c_row.saturating_sub(between_prev_row);
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
                        let gap_start = comma_end_byte;
                        let gap_end = c.start_byte().min(self.src.len());
                        let is_line_comment = c.kind() == "line_comment";
                        let count = if gap_start < gap_end {
                            self.src[gap_start..gap_end]
                                .iter()
                                .filter(|&&b| b == b' ' || b == b'\t')
                                .count()
                        } else {
                            0
                        };
                        let spaces = if is_line_comment {
                            count as u32
                        } else {
                            count.max(1) as u32
                        };
                        if spaces > 0 {
                            Spacing::spaces(spaces, spaces)
                        } else {
                            Spacing::none()
                        }
                    };
                    let comment_block = self.comment_leaf(*c).with_spacing(spacing);
                    let comment_block = if on_own_line {
                        comment_block.with_indent(arg_indent).with_arg_probe()
                    } else {
                        comment_block
                    };
                    children.push(comment_block);
                    between_prev_row = c.end_position().row;
                }
            }

            ArgumentPlacement::Wrapped
        }
    }
}
