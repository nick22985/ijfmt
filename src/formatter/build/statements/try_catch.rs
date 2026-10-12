use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::ir::*;

impl<'a> BlockBuilder<'a> {
    pub(crate) fn catch_clause(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let all_children: Vec<Node> = node.children(&mut cursor).collect();
        let mut prev_end_byte: Option<usize> = None;

        let lparen_node = all_children.iter().find(|c| c.kind() == "(");
        let param_node = all_children
            .iter()
            .find(|c| c.kind() == "catch_formal_parameter");

        let param_on_new_row = match (lparen_node, param_node) {
            (Some(lp), Some(fp)) => fp.start_position().row > lp.end_position().row,
            _ => false,
        };

        let is_multi_catch = param_node.is_some_and(|fp| {
            let mut c = fp.walk();
            fp.children(&mut c).any(|ch| {
                ch.kind() == "catch_type" && {
                    let mut cc = ch.walk();
                    ch.children(&mut cc).any(|t| t.kind() == "|")
                }
            })
        });

        let param_indent = if param_on_new_row || !is_multi_catch {
            IndentType::Continuation
        } else if let (Some(_lp), Some(fp)) = (lparen_node, param_node) {
            let brace_prefix = if self.cfg.catch_on_new_line { 0 } else { 2 };
            let space = if self.cfg.space_before_catch_parentheses {
                1
            } else {
                0
            };
            let mut mods = 0usize;
            let mut c = fp.walk();
            for ch in fp.children(&mut c) {
                if ch.kind() == "catch_type" {
                    break;
                }
                mods += self.text(ch).chars().count() + 1;
            }
            IndentType::Spaces(brace_prefix + 5 + space + 1 + mods)
        } else {
            IndentType::Continuation
        };

        for &child in &all_children {
            let kind = child.kind();
            let block = if kind == "catch_formal_parameter" {
                let param_wrap = if !is_multi_catch && !param_on_new_row {
                    let w = self.ids.wrap(WrapType::Normal);
                    self.wraps.insert(w.id, w.clone());
                    self.positional_wraps.insert(w.id);
                    Some(w)
                } else {
                    None
                };
                let mut b = self
                    .catch_formal_parameter(child, param_wrap.clone())
                    .with_indent(param_indent);
                if let Some(w) = param_wrap {
                    b = b.with_wrap(w);
                }
                b
            } else if kind == "block" {
                self.code_block(child)
            } else if child.is_extra() {
                self.comment_leaf(child)
            } else {
                self.node(child)
            };

            let spacing = if prev_end_byte.is_none() {
                Spacing::none()
            } else if kind == "(" {
                if self.cfg.space_before_catch_parentheses {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                }
            } else if kind == "catch_formal_parameter" {
                if param_on_new_row {
                    let row_gap = lparen_node.map_or(1, |lp| {
                        child
                            .start_position()
                            .row
                            .saturating_sub(lp.end_position().row)
                    });
                    let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1);
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: capped.max(1) as u32,
                        keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                        keep_line_breaks: true,
                    }
                } else {
                    Spacing::none()
                }
            } else if kind == ")" {
                Spacing::none()
            } else if kind == "block" {
                use crate::config::BraceStyle;
                match self.cfg.method_brace_style {
                    BraceStyle::NextLine => self.lbrace_new_line(child),
                    BraceStyle::EndOfLine => Spacing::one_space(),
                }
            } else {
                Spacing::one_space()
            };

            let block = if prev_end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };
            children.push(block);
            prev_end_byte = Some(child.end_byte());
        }

        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        }
    }

    fn catch_formal_parameter(&mut self, node: Node, mods_wrap: Option<Wrap>) -> Block {
        let mut children = Vec::new();
        let wrap = self.ids.wrap(WrapType::Normal);
        self.wraps.insert(wrap.id, wrap.clone());
        let mut first = true;
        let mut stack: Vec<Node> = {
            let mut c = node.walk();
            node.children(&mut c).collect::<Vec<_>>()
        };
        let mut flat: Vec<Node> = Vec::new();
        for ch in stack.drain(..) {
            if ch.kind() == "catch_type" {
                let mut c = ch.walk();
                flat.extend(ch.children(&mut c));
            } else {
                flat.push(ch);
            }
        }
        let mut prev_end_row: Option<usize> = None;
        let mut prev_was_modifiers = false;
        for ch in flat {
            let follows_modifiers = prev_was_modifiers && !ch.is_extra();
            let mut block = if ch.is_extra() {
                self.comment_leaf(ch)
            } else {
                self.node(ch)
            };
            if follows_modifiers && let Some(w) = &mods_wrap {
                block = block
                    .with_wrap(w.clone())
                    .with_indent(IndentType::CancelOneContinuation);
            }
            prev_was_modifiers = ch.kind() == "modifiers";
            if !first {
                let on_new_row = prev_end_row.is_some_and(|pr| ch.start_position().row > pr);
                if on_new_row {
                    block = block.with_spacing(Spacing::new_line());
                } else {
                    block = block.with_spacing(Spacing::one_space());
                }
            }
            let is_exception_type = ch.kind() != "|"
                && ch
                    .parent()
                    .is_some_and(|parent| parent.kind() == "catch_type");
            if is_exception_type {
                block = block.with_wrap(if follows_modifiers {
                    mods_wrap.clone().unwrap_or_else(|| wrap.clone())
                } else {
                    wrap.clone()
                });
            }
            first = false;
            prev_end_row = Some(ch.end_position().row);
            children.push(block);
        }
        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        }
    }
    pub(in crate::formatter::build) fn resource(&mut self, node: Node) -> Block {
        let mut block = self.fallback(node);
        let mut saw_eq = false;
        for child in block.children.iter_mut() {
            if saw_eq && child.source_has_line_break_before && child.indent == IndentType::None {
                child.indent = IndentType::Continuation;
            }
            if child.text == "=" {
                saw_eq = true;
            }
        }
        block
    }

    pub(in crate::formatter::build) fn resource_specification(&mut self, node: Node) -> Block {
        let mut parts = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_end_row: Option<usize> = None;
        let use_alignment = self.cfg.align_multiline_resources;
        let alignment_id = if use_alignment {
            Some(self.ids.alignment_id())
        } else {
            None
        };
        let mut is_first_resource = true;
        for child in node.children(&mut cursor) {
            if child.is_extra() {
                parts.push(self.comment_leaf(child).with_spacing(Spacing::one_space()));
                prev_end_byte = Some(child.end_byte());
                prev_end_row = Some(child.end_position().row);
                continue;
            }
            let block = if child.kind() == "(" || child.kind() == ")" {
                Block::leaf(self.text(child))
            } else if child.kind() == ";" {
                Block::leaf(";")
            } else {
                self.node(child)
            };
            let prev_was_open = parts.last().is_some_and(|b: &Block| b.text == "(");
            let is_resource = child.kind() == "resource";
            let spacing = if prev_end_byte.is_none() {
                Spacing::none()
            } else if child.kind() == ")" {
                let rparen_on_own_line =
                    prev_end_row.is_some_and(|pr| child.start_position().row > pr);
                if rparen_on_own_line {
                    Spacing::new_line()
                } else {
                    Spacing::none()
                }
            } else if child.kind() == ";" || child.kind() == "(" {
                Spacing::none()
            } else if prev_was_open {
                let source_break = prev_end_row.is_some_and(|pr| child.start_position().row > pr);
                if source_break {
                    Spacing::new_line()
                } else {
                    Spacing::none()
                }
            } else {
                let source_break = prev_end_row.is_some_and(|pr| child.start_position().row > pr);
                if source_break {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                }
            };
            let block = if prev_end_byte.is_some() {
                let block = block.with_spacing(spacing);
                let on_new_line = prev_end_row.is_some_and(|pr| child.start_position().row > pr);
                if is_resource && on_new_line {
                    if let Some(aid) = alignment_id {
                        block
                            .with_indent(IndentType::Continuation)
                            .with_alignment(aid)
                            .with_res_align()
                    } else {
                        block.with_indent(IndentType::Continuation)
                    }
                } else if is_resource && is_first_resource {
                    if let Some(aid) = alignment_id {
                        block.with_alignment(aid)
                    } else {
                        block
                    }
                } else {
                    block
                }
            } else {
                block
            };
            if is_resource {
                is_first_resource = false;
            }
            prev_end_byte = Some(child.end_byte());
            prev_end_row = Some(child.end_position().row);
            parts.push(block);
        }
        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Block::node(parts)
        }
    }
}
