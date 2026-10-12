use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::ir::*;
use std::collections::HashMap;

impl<'a> BlockBuilder<'a> {
    pub(crate) fn code_block(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut prev_end_row: Option<usize> = None;
        let mut prev_kind: Option<String> = None;
        let mut prev_ends_with_colon = false;

        let all_children: Vec<Node> = {
            let mut c = node.walk();
            node.children(&mut c).collect()
        };

        let (local_var_align, local_var_cont_align) = if self.cfg.align_group_field_declarations
            || self.cfg.align_consecutive_variable_declarations
        {
            let stmts: Vec<Node> = all_children
                .iter()
                .filter(|c| !matches!(c.kind(), "{" | "}"))
                .copied()
                .collect();
            self.compute_local_var_group_alignment(&stmts)
        } else {
            (HashMap::new(), HashMap::new())
        };

        let inline_comment_body = {
            let inner: Vec<&Node> = all_children
                .iter()
                .filter(|c| !matches!(c.kind(), "{" | "}"))
                .collect();
            !inner.is_empty()
                && inner.iter().all(|c| c.kind() == "block_comment")
                && match (all_children.first(), all_children.last()) {
                    (Some(f), Some(l)) => {
                        f.kind() == "{"
                            && l.kind() == "}"
                            && f.start_position().row == l.end_position().row
                    }
                    _ => false,
                }
        };

        let mut i = 0;
        while i < all_children.len() {
            let child = all_children[i];
            let is_open = child.kind() == "{";
            let is_close = child.kind() == "}";

            let is_inline_with_brace = prev_kind.as_deref() == Some("{")
                && prev_end_row.is_some_and(|pr| child.start_position().row == pr)
                && child.is_extra();

            let is_force_brace_candidate = matches!(
                child.kind(),
                "if_statement"
                    | "for_statement"
                    | "enhanced_for_statement"
                    | "while_statement"
                    | "do_statement"
            );
            let next_is_trailing_comment = is_force_brace_candidate
                && i + 1 < all_children.len()
                && all_children[i + 1].is_extra()
                && all_children[i + 1].start_position().row == child.end_position().row;

            let saved_trailing = if next_is_trailing_comment {
                Some((
                    std::mem::take(&mut self.force_brace_trailing_comments),
                    self.force_brace_trailing_owner.take(),
                ))
            } else {
                None
            };
            if next_is_trailing_comment {
                let comment_node = all_children[i + 1];
                let pb = child.end_byte();
                let ge = comment_node.start_byte().min(self.src.len());
                let ts = self.cfg.indent_size.max(1) as u32;
                let sp = if pb < ge {
                    self.src[pb..ge]
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
                        .sum::<u32>()
                } else {
                    0
                };
                self.force_brace_trailing_comments.push(
                    self.comment_leaf(comment_node)
                        .with_spacing(Spacing::spaces(sp, sp)),
                );
                self.force_brace_trailing_owner = Some(child.id());
            }

            let mut set_formatter_off_after = false;
            if child.is_extra() && child.kind() == "line_comment" {
                let comment_text = self.text(child);
                if comment_text.contains("@formatter:off") {
                    set_formatter_off_after = true;
                } else if comment_text.contains("@formatter:on") {
                }
            }

            let is_formatter_on = self.formatter_off
                && child.is_extra()
                && child.kind() == "line_comment"
                && self.text(child).contains("@formatter:on");
            let block = if is_open {
                Block::leaf("{")
            } else if is_close {
                Block::leaf("}")
            } else if self.formatter_off && !set_formatter_off_after {
                self.raw_source_block(child)
            } else if is_inline_with_brace {
                self.comment_leaf(child)
            } else if child.is_extra() {
                let at_col_0 = child.start_position().column == 0;
                if at_col_0 {
                    self.comment_leaf(child)
                        .with_statement_comment()
                        .with_indent(IndentType::ColumnZero)
                } else {
                    self.comment_leaf(child)
                        .with_statement_comment()
                        .with_indent(IndentType::Normal)
                }
            } else if child.kind() == "local_variable_declaration" {
                if let Some(&(mp, tp, np)) = local_var_align.get(&child.id()) {
                    self.field_declaration_aligned(child, mp, tp, np, &local_var_cont_align)
                        .with_indent(IndentType::Normal)
                } else {
                    self.node(child).with_indent(IndentType::Normal)
                }
            } else {
                self.node(child).with_indent(IndentType::Normal)
            };

            if next_is_trailing_comment {
                if self.force_brace_trailing_comments.is_empty() {
                    i += 1;
                } else {
                    self.force_brace_trailing_comments.clear();
                    self.force_brace_trailing_owner = None;
                }
                if let Some((v, o)) = saved_trailing {
                    self.force_brace_trailing_comments = v;
                    self.force_brace_trailing_owner = o;
                }
            }

            let spacing = if let Some(prev_row) = prev_end_row {
                let row_gap = child.start_position().row.saturating_sub(prev_row);
                if is_inline_with_brace {
                    let prev_end_byte_val = all_children[i.saturating_sub(1)].end_byte();
                    let gap = &self.src[prev_end_byte_val..child.start_byte().min(self.src.len())];
                    let ts = self.cfg.indent_size.max(1) as u32;
                    let spaces = gap
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
                    Spacing::spaces(spaces, spaces)
                } else if is_close && inline_comment_body {
                    let prev_end_byte_val = all_children[i.saturating_sub(1)].end_byte();
                    let gap_end = child.start_byte().min(self.src.len());
                    let spaces = if prev_end_byte_val < gap_end {
                        self.src[prev_end_byte_val..gap_end]
                            .iter()
                            .filter(|&&b| b == b' ' || b == b'\t')
                            .count()
                            .min(1) as u32
                    } else {
                        0
                    };
                    Spacing::spaces(spaces, spaces)
                } else if is_close {
                    let capped = row_gap
                        .min(self.cfg.keep_blank_lines_before_rbrace + 1)
                        .max(1);
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: capped as u32,
                        keep_blank_lines: self.cfg.keep_blank_lines_before_rbrace as u32,
                        keep_line_breaks: true,
                    }
                } else if prev_ends_with_colon {
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: 0,
                        keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                        keep_line_breaks: true,
                    }
                } else if row_gap >= 2 {
                    let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1);
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: capped as u32,
                        keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                        keep_line_breaks: true,
                    }
                } else if row_gap >= 1 || prev_kind.as_deref() == Some("{") || child.kind() == ";" {
                    Spacing::new_line()
                } else if child.is_extra() {
                    let prev_end_byte_val = all_children[i.saturating_sub(1)].end_byte();
                    let gap_bytes = child.start_byte().min(self.src.len());
                    if prev_end_byte_val < gap_bytes {
                        let gap = &self.src[prev_end_byte_val..gap_bytes];
                        let ts = self.cfg.indent_size.max(1) as u32;
                        let spaces = gap
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
                        Spacing::spaces(spaces, spaces)
                    } else {
                        Spacing::none()
                    }
                } else if !child.is_extra() {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                }
            } else {
                Spacing::none()
            };

            let block = if prev_end_row.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };

            if set_formatter_off_after {
                self.formatter_off = true;
            }
            if is_formatter_on {
                self.formatter_off = false;
            }

            prev_end_row = Some(child.end_position().row);
            prev_kind = Some(child.kind().to_string());
            prev_ends_with_colon =
                matches!(child.kind(), "ERROR" | ":") && self.text(child).trim_end().ends_with(':');
            children.push(block);
            i += 1;
        }

        Block::node(children)
    }
}
