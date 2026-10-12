use tree_sitter::Node;

use crate::config::BraceStyle;
use crate::formatter::build::{BlockBuilder, util};
use crate::formatter::ir::*;

impl<'a> BlockBuilder<'a> {
    pub(crate) fn switch_block(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_row: Option<usize> = None;
        let mut prev_kind: Option<String> = None;
        let mut prev_comment_at_case_level: Option<bool> = None;

        let mut prev_end_byte: Option<usize> = None;

        for child in node.children(&mut cursor) {
            let is_open = child.kind() == "{";
            let is_close = child.kind() == "}";

            let is_inline_with_brace = prev_kind.as_deref() == Some("{")
                && prev_end_row.is_some_and(|pr| child.start_position().row == pr)
                && child.is_extra();

            let block = if is_open {
                Block::leaf("{")
            } else if is_close {
                Block::leaf("}")
            } else if is_inline_with_brace {
                prev_comment_at_case_level = Some(true);
                self.comment_leaf(child)
            } else if child.kind() == "switch_block_statement_group" {
                self.switch_block_statement_group(child)
                    .with_indent(IndentType::Normal)
            } else if child.kind() == "switch_rule" {
                self.switch_rule(child).with_indent(IndentType::Normal)
            } else if child.is_extra() {
                let at_case_level = if prev_kind.as_deref() == Some("line_comment")
                    || prev_kind.as_deref() == Some("block_comment")
                {
                    prev_comment_at_case_level.unwrap_or(false)
                } else if matches!(prev_kind.as_deref(), Some("{" | "switch_rule")) {
                    true
                } else if prev_kind.as_deref() == Some("switch_block_statement_group") {
                    let mut prev_group = None;
                    let mut wc = node.walk();
                    for sibling in node.children(&mut wc) {
                        if sibling.id() == child.id() {
                            break;
                        }
                        if sibling.kind() == "switch_block_statement_group" {
                            prev_group = Some(sibling);
                        }
                    }
                    prev_group.is_some_and(|g| {
                        let mut gc = g.walk();
                        g.children(&mut gc)
                            .filter(|c| {
                                !c.is_extra() && c.kind() != "switch_label" && c.kind() != ":"
                            })
                            .any(|s| {
                                matches!(s.kind(), "break_statement" | "return_statement" | "block")
                            })
                    })
                } else {
                    false
                };
                prev_comment_at_case_level = Some(at_case_level);
                if at_case_level {
                    self.comment_leaf(child).with_indent(IndentType::Normal)
                } else {
                    let comment = self.comment_leaf(child).with_indent(IndentType::Normal);
                    Block::node(vec![comment]).with_indent(IndentType::Normal)
                }
            } else {
                self.node(child).with_indent(IndentType::Normal)
            };

            let spacing = if let Some(prev_row) = prev_end_row {
                let row_gap = child.start_position().row.saturating_sub(prev_row);
                if is_inline_with_brace {
                    let gap_start = prev_end_byte.unwrap_or(0);
                    let gap = &self.src[gap_start..child.start_byte().min(self.src.len())];
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
                } else if row_gap >= 2 {
                    let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1);
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: capped as u32,
                        keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                        keep_line_breaks: true,
                    }
                } else if row_gap >= 1
                    || prev_kind.as_deref() == Some("{")
                    || (child.kind() == "switch_block_statement_group"
                        && prev_kind.as_deref() == Some("switch_block_statement_group"))
                    || (child.kind() == "switch_rule"
                        && prev_kind.as_deref() == Some("switch_rule"))
                {
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

            prev_end_row = Some(child.end_position().row);
            prev_end_byte = Some(child.end_byte());
            prev_kind = Some(child.kind().to_string());
            children.push(block);
        }

        Block::node(children)
    }

    fn switch_block_statement_group(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_row: Option<usize> = None;
        let mut prev_kind: Option<String> = None;
        let mut prev_end_byte_for_comment: Option<usize> = None;

        for child in node.children(&mut cursor) {
            let is_label = child.kind() == "switch_label";
            let is_colon = child.kind() == ":";

            let block = if is_label {
                self.node(child)
            } else if is_colon {
                Block::leaf(":")
            } else if child.is_extra() {
                self.comment_leaf(child).with_indent(IndentType::Normal)
            } else if child.kind() == "block" || prev_kind.as_deref() == Some("block") {
                self.node(child)
            } else {
                self.node(child).with_indent(IndentType::Normal)
            };

            let spacing = if let Some(prev_row) = prev_end_row {
                let row_gap = child.start_position().row.saturating_sub(prev_row);
                if is_colon {
                    Spacing::none()
                } else if row_gap >= 2 {
                    let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1);
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: capped as u32,
                        keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                        keep_line_breaks: true,
                    }
                } else if child.is_extra() && row_gap == 0 && prev_kind.as_deref() == Some(":") {
                    let pb = prev_end_byte_for_comment.unwrap_or(0);
                    let ge = child.start_byte().min(self.src.len());
                    let ts = self.cfg.indent_size.max(1) as u32;
                    let spaces = if pb < ge {
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
                    Spacing::spaces(spaces, spaces)
                } else if row_gap >= 1
                    || prev_kind.as_deref() == Some(":")
                    || prev_kind.as_deref() == Some("switch_label")
                    || is_label
                {
                    if child.kind() == "block"
                        && prev_kind.as_deref() == Some(":")
                        && self.cfg.brace_style == crate::config::BraceStyle::EndOfLine
                    {
                        Spacing::one_space()
                    } else {
                        Spacing::new_line()
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

            prev_end_row = Some(child.end_position().row);
            prev_kind = Some(child.kind().to_string());
            prev_end_byte_for_comment = Some(child.end_byte());
            children.push(block);
        }

        Block::node(children)
    }

    fn switch_rule(&mut self, node: Node) -> Block {
        if let Some(block) = self.recovered_switch_lambda_rule(node) {
            return block;
        }

        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut seen_arrow = false;
        let mut prev_end_row: Option<usize> = None;

        for child in node.children(&mut cursor) {
            let kind = child.kind();
            let is_arrow = kind == "->";

            let block = if kind == "switch_label" {
                self.node(child)
            } else if is_arrow {
                Block::leaf("->")
            } else if child.is_extra() {
                let comment = self.comment_leaf(child);
                if seen_arrow && prev_end_row.is_some_and(|row| child.start_position().row > row) {
                    comment.with_indent(IndentType::Normal)
                } else {
                    comment
                }
            } else if seen_arrow && kind != "block" {
                let w = Wrap {
                    id: self.ids.wrap_id(),
                    wrap_type: WrapType::Normal,
                    wrap_first_element: true,
                    parent: None,
                    ignore_parent_wraps: false,
                    cand_ignore_parents: false,
                };
                self.wraps.insert(w.id, w.clone());
                self.node(child)
                    .with_wrap(w)
                    .with_indent(IndentType::ContinuationIfWrapped(usize::MAX))
            } else {
                self.node(child)
            };

            let spacing = if prev_end_row.is_some() {
                if child.is_extra()
                    && seen_arrow
                    && prev_end_row.is_some_and(|row| child.start_position().row > row)
                {
                    Spacing::new_line()
                } else if seen_arrow
                    && kind == "block"
                    && self.cfg.brace_style == BraceStyle::NextLine
                {
                    self.lbrace_new_line(child)
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

            if is_arrow {
                seen_arrow = true;
            }
            prev_end_row = Some(child.end_position().row);
            children.push(block);
        }

        Block::node(children)
    }

    fn recovered_switch_lambda_rule(&mut self, node: Node) -> Option<Block> {
        let mut cursor = node.walk();
        let ast: Vec<Node> = node.children(&mut cursor).collect();
        let label = *ast.first()?;
        if label.kind() != "switch_label" || ast.get(1)?.kind() != "->" {
            return None;
        }
        let lambda = label
            .named_child_count()
            .checked_sub(1)
            .and_then(|i| label.named_child(i as u32))?;
        if lambda.kind() != "lambda_expression" {
            return None;
        }

        let mut label_block = self.node(label);
        let lambda_block = label_block.children.last_mut()?;
        let mut parameter = lambda_block.children.pop()?;
        parameter.spacing = None;
        parameter.break_bonus = false;

        let wrap = Wrap {
            id: self.ids.wrap_id(),
            wrap_type: WrapType::Normal,
            wrap_first_element: true,
            parent: None,
            ignore_parent_wraps: true,
            cand_ignore_parents: false,
        };
        self.wraps.insert(wrap.id, wrap.clone());

        let previous_enclosing = self.enclosing_arg_wrap;
        self.enclosing_arg_wrap = Some(wrap.id);
        let mut body_parts = vec![
            parameter,
            Block::leaf("->").with_spacing(Spacing::one_space()),
        ];
        let mut previous_row = ast[1].end_position().row;
        for child in ast.iter().skip(2).copied() {
            let spacing = if child.start_position().row > previous_row {
                Spacing::new_line()
            } else {
                Spacing::one_space()
            };
            let block = if child.is_extra() {
                self.comment_leaf(child)
            } else {
                self.node(child)
            };
            body_parts.push(block.with_spacing(spacing));
            previous_row = child.end_position().row;
        }
        self.enclosing_arg_wrap = previous_enclosing;

        let source_break = lambda
            .child_by_field_name("body")
            .is_some_and(|body| body.start_position().row > ast[1].start_position().row);
        let spacing = if source_break {
            Spacing::new_line()
        } else {
            Spacing::one_space()
        };
        let body = Block::node(body_parts)
            .with_spacing(spacing)
            .with_wrap(wrap)
            .with_indent(IndentType::ContinuationIfWrapped(usize::MAX));
        Some(Block::node(vec![label_block, body]))
    }

    pub(in crate::formatter::build) fn switch_label(&mut self, node: Node) -> Block {
        if util::subtree_has_error(node) || util::has_uncovered_source(node, self.src) {
            let txt = self.text(node);
            if !txt.contains('\n') && !txt.contains("  ") {
                let split = txt.strip_prefix("case ").map(|r| r.to_string());
                if let Some(rest) = split.as_deref()
                    && let Some(open) = rest.find('(')
                {
                    let inner = &rest[open + 1..];
                    if rest.ends_with(')')
                        && !inner.is_empty()
                        && inner.len() > 1
                        && !rest[..open].contains(' ')
                    {
                        let inner = &inner[..inner.len() - 1];
                        let wrap_id = self.ids.wrap_id();
                        let mk = |first: bool| Wrap {
                            id: wrap_id,
                            wrap_type: WrapType::ChopIfNeeded,
                            wrap_first_element: first,
                            parent: None,
                            ignore_parent_wraps: false,
                            cand_ignore_parents: false,
                        };
                        self.wraps.insert(wrap_id, mk(true));
                        return Block::node(vec![
                            Block::leaf("case"),
                            Block::leaf(format!("{}(", &rest[..open]))
                                .with_spacing(Spacing::one_space()),
                            Block::leaf(inner.to_string())
                                .with_spacing(Spacing::none())
                                .with_wrap(mk(true))
                                .with_indent(IndentType::Continuation),
                            Block::leaf(")")
                                .with_spacing(Spacing::none())
                                .with_wrap(mk(false)),
                        ]);
                    }
                }
                if let Some(rest) = split {
                    let w = Wrap {
                        id: self.ids.wrap_id(),
                        wrap_type: WrapType::Normal,
                        wrap_first_element: true,
                        parent: None,
                        ignore_parent_wraps: false,
                        cand_ignore_parents: false,
                    };
                    self.wraps.insert(w.id, w.clone());
                    return Block::node(vec![
                        Block::leaf("case"),
                        Block::leaf(rest)
                            .with_spacing(Spacing::one_space())
                            .with_wrap(w)
                            .with_indent(IndentType::Continuation),
                    ]);
                }
                return Block::leaf(txt.to_string());
            }
        }
        let has_case_kw = {
            let mut c = node.walk();
            node.children(&mut c).any(|ch| ch.kind() == "case")
        };
        let label_wrap = if has_case_kw {
            let w = Wrap {
                id: self.ids.wrap_id(),
                wrap_type: WrapType::Normal,
                wrap_first_element: false,
                parent: None,
                ignore_parent_wraps: false,
                cand_ignore_parents: false,
            };
            self.wraps.insert(w.id, w.clone());
            self.positional_wraps.insert(w.id);
            Some((w, self.ids.alignment_id()))
        } else {
            None
        };
        let mut seen_case_kw = false;
        let mut prev_was_case_kw = false;
        let mut parts = Vec::new();
        let mut cursor = node.walk();
        let mut prev_text_ends_with_dot = false;
        let mut prev_end_row: Option<usize> = None;
        for child in node.children(&mut cursor) {
            if child.is_extra() {
                if child.kind() == "ERROR" {
                    let block = self.fallback(child);
                    let spacing = if parts.is_empty() {
                        Spacing::none()
                    } else {
                        Spacing::one_space()
                    };
                    parts.push(if !parts.is_empty() {
                        block.with_spacing(spacing)
                    } else {
                        block
                    });
                    prev_text_ends_with_dot = self.text(child).ends_with('.');
                } else {
                    parts.push(self.comment_leaf(child).with_spacing(Spacing::one_space()));
                    prev_text_ends_with_dot = false;
                }
                prev_end_row = Some(child.end_position().row);
                prev_was_case_kw = false;
                continue;
            }
            let is_label_element =
                seen_case_kw && !matches!(child.kind(), "," | ":" | "." | "case" | "default");
            let block = self.node(child);
            let prev_kind_case = prev_was_case_kw;
            let spacing = if matches!(child.kind(), ":" | "," | ".")
                || parts.is_empty()
                || prev_text_ends_with_dot
            {
                Spacing::none()
            } else if is_label_element && prev_kind_case {
                Spacing::one_space()
            } else if is_label_element
                && prev_end_row.is_some_and(|r| child.start_position().row > r)
            {
                Spacing {
                    min_spaces: 1,
                    max_spaces: 1,
                    min_line_feeds: 1,
                    keep_blank_lines: 0,
                    keep_line_breaks: true,
                }
            } else {
                Spacing::one_space()
            };
            let block = if !parts.is_empty() {
                block.with_spacing(spacing)
            } else {
                block
            };
            let block = match (&label_wrap, is_label_element) {
                (Some((w, aid)), true) if child.kind() != "pattern" => block
                    .with_indent(IndentType::Continuation)
                    .with_wrap(w.clone())
                    .with_alignment(*aid),
                _ => block,
            };
            if child.kind() == "case" {
                seen_case_kw = true;
            }
            prev_was_case_kw = child.kind() == "case";
            parts.push(block);
            prev_text_ends_with_dot = self.text(child).ends_with('.');
            prev_end_row = Some(child.end_position().row);
        }
        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Block::node(parts)
        }
    }
}
