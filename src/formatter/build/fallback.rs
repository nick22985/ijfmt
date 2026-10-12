use tree_sitter::Node;

use super::super::ir::*;
use super::expressions::operations;
use super::{BlockBuilder, util};

impl<'a> BlockBuilder<'a> {
    pub(crate) fn fallback(&mut self, node: Node) -> Block {
        if node.child_count() == 0 {
            return Block::leaf(self.text(node).to_string()).with_source_offset(node.start_byte());
        }

        fn is_no_space_before(kind: &str) -> bool {
            matches!(
                kind,
                "," | ";"
                    | "."
                    | ")"
                    | "]"
                    | "..."
                    | "argument_list"
                    | "record_pattern_body"
                    | "type_arguments"
                    | "type_parameters"
            )
        }

        fn is_plain_dimensions(child: Node) -> bool {
            child.kind() == "dimensions"
                && !child
                    .children(&mut child.walk())
                    .any(|c| matches!(c.kind(), "marker_annotation" | "annotation"))
        }

        fn is_space_after(kind: &str) -> bool {
            matches!(kind, "," | ";" | "..." | "assert")
        }

        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_row: Option<usize> = None;
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_kind: Option<String> = None;
        let mut jump_value_pending = false;

        let array_access_in_conditional =
            node.kind() == "array_access" && operations::binary_inside_conditional(node);

        for child in node.children(&mut cursor) {
            let mut block = if child.is_extra() {
                self.comment_leaf(child)
            } else {
                self.node(child)
            };

            if matches!(
                node.kind(),
                "formal_parameter" | "spread_parameter" | "catch_formal_parameter"
            ) && prev_end_row.is_some()
                && prev_kind.as_deref() != Some("modifiers")
            {
                block = block.with_param_probe();
            }

            if node.kind() == "array_access"
                && node
                    .child_by_field_name("index")
                    .is_some_and(|ix| ix.id() == child.id())
            {
                block = if array_access_in_conditional {
                    block.with_tern_bin_probe()
                } else {
                    block.with_paren_cwf_probe()
                };
            }

            if node.kind() == "dimensions_expr" && !child.is_extra() {
                let k = child.kind();
                if k != "[" && k != "]" {
                    block = block
                        .with_paren_cwf_probe()
                        .with_indent(IndentType::Continuation);
                }
            }

            if let Some(prev_row) = prev_end_row {
                let prev_byte = prev_end_byte.unwrap_or(0);
                let kind = child.kind();

                let is_brace_block = kind == "block" || kind == "switch_block";
                let force_newline_for_brace =
                    is_brace_block && self.cfg.brace_style == crate::config::BraceStyle::NextLine;
                let force_sameline_for_brace = is_brace_block
                    && self.cfg.brace_style == crate::config::BraceStyle::EndOfLine
                    && child.start_position().row > prev_row
                    && prev_kind.as_deref() != Some("line_comment");

                let stmt_semi_gap = if kind == ";"
                    && matches!(
                        node.kind(),
                        "return_statement" | "throw_statement" | "yield_statement"
                    )
                    && self.cfg.keep_blank_lines_in_code > 0
                {
                    let gap = child.start_position().row.saturating_sub(prev_row);
                    if gap >= 2 {
                        Some(gap.min(self.cfg.keep_blank_lines_in_code + 1))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let blank_keep = |capped: usize, keep: usize| Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: capped as u32,
                    keep_blank_lines: keep as u32,
                    keep_line_breaks: true,
                };

                if force_newline_for_brace {
                    block = block.with_spacing(self.lbrace_new_line(child));
                } else if force_sameline_for_brace {
                    block = block.with_spacing(Spacing::one_space());
                } else if child.start_position().row > prev_row {
                    block = block
                        .with_spacing(match stmt_semi_gap {
                            Some(capped) => blank_keep(capped, self.cfg.keep_blank_lines_in_code),
                            None => Spacing::new_line(),
                        })
                        .with_source_line_break();
                    if node.kind() == "inferred_parameters" {
                        block = block.with_indent(if util::is_glued_lambda_param_list(node) {
                            IndentType::LambdaParamGlued
                        } else {
                            IndentType::Continuation
                        });
                    }
                    if kind == ";"
                        && matches!(
                            node.kind(),
                            "return_statement" | "throw_statement" | "yield_statement"
                        )
                    {
                        block = block.with_indent(IndentType::Continuation);
                    }
                    if matches!(node.kind(), "type_arguments" | "type_parameters") && kind != "<" {
                        block = block.with_indent(IndentType::Continuation);
                    }
                } else {
                    let catch_paren = node.kind() == "catch_clause"
                        && prev_kind.as_deref() == Some("catch")
                        && (kind == "(" || kind == "catch_formal_parameter");

                    let spacing = if catch_paren {
                        if self.cfg.space_before_catch_parentheses {
                            Spacing::one_space()
                        } else {
                            Spacing::none()
                        }
                    } else if is_brace_block {
                        Spacing::one_space()
                    } else if (is_no_space_before(kind)
                        && !(kind == "..." && ends_with_annotation(self.src, prev_byte)))
                        || is_plain_dimensions(child)
                        || node.kind() == "update_expression"
                        || (node.kind() == "unary_expression"
                            && matches!(prev_kind.as_deref(), Some("!" | "~" | "-" | "+")))
                        || (node.kind() == "array_access"
                            && (kind == "[" || prev_kind.as_deref() == Some("[")))
                    {
                        Spacing::none()
                    } else if kind == "throws"
                        || matches!(prev_kind.as_deref(), Some("return" | "throw" | "yield"))
                    {
                        Spacing::one_space()
                    } else if kind == "annotation_argument_list" {
                        Spacing::none()
                    } else if prev_kind.as_deref().is_some_and(is_space_after)
                        || prev_kind.as_deref() == Some("modifiers")
                        || (matches!(
                            prev_kind.as_deref(),
                            Some("marker_annotation" | "annotation")
                        ) && matches!(kind, "[" | "..."))
                    {
                        Spacing::one_space()
                    } else if self
                        .src
                        .get(prev_byte.saturating_sub(1))
                        .is_some_and(|byte| matches!(byte, b'.' | b'<'))
                    {
                        Spacing::none()
                    } else if child.start_byte() > prev_byte {
                        let gap = &self.src[prev_byte..child.start_byte()];
                        if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                            if child.is_extra()
                                && child.kind() == "line_comment"
                                && child.start_position().row == prev_end_row.unwrap_or(usize::MAX)
                            {
                                let spaces =
                                    gap.iter().filter(|&&b| b == b' ').count().max(1) as u32;
                                Spacing::spaces(spaces, spaces)
                            } else {
                                Spacing::one_space()
                            }
                        } else {
                            Spacing::none()
                        }
                    } else {
                        Spacing::none()
                    };
                    block = block.with_spacing(spacing);
                }
            }

            if jump_value_pending && child.kind() != ";" {
                block = block.with_break_bonus();
            }

            if matches!(child.kind(), "return" | "throw" | "yield")
                && matches!(
                    node.kind(),
                    "return_statement" | "throw_statement" | "yield_statement"
                )
            {
                jump_value_pending = true;
            } else if jump_value_pending && !child.is_extra() {
                jump_value_pending = false;
            }

            prev_end_row = Some(child.end_position().row);
            prev_end_byte = Some(child.end_byte());
            prev_kind = Some(child.kind().to_string());
            children.push(block);
        }

        if children.len() == 1 {
            children.pop().unwrap()
        } else if matches!(
            node.kind(),
            "formal_parameter" | "spread_parameter" | "catch_formal_parameter"
        ) {
            Block::node(children).with_param_scope()
        } else if array_access_in_conditional {
            Block::node(children).with_bin_scope()
        } else {
            Block::node(children)
        }
    }
}

fn ends_with_annotation(src: &[u8], end: usize) -> bool {
    let mut p = end.min(src.len());
    while p > 0
        && (src[p - 1].is_ascii_alphanumeric()
            || src[p - 1] == b'_'
            || src[p - 1] == b'$'
            || src[p - 1] == b'.')
    {
        p -= 1;
    }
    p > 0 && p < end && src[p - 1] == b'@'
}
