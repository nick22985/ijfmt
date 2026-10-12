use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::ir::*;

impl<'a> BlockBuilder<'a> {
    pub(in crate::formatter::build) fn field_access(&mut self, node: Node) -> Block {
        let mut block = self.fallback(node);
        let mut past_dot = false;
        for child in block.children.iter_mut() {
            if child.text == "." && child.source_has_line_break_before {
                past_dot = true;
            }
            if past_dot && child.indent == IndentType::None {
                child.indent = IndentType::Continuation;
            }
        }
        block
    }

    pub(in crate::formatter::build) fn parenthesized_expression(&mut self, node: Node) -> Block {
        let lparen_row = node.start_position().row;
        let mut has_content_on_new_line = false;
        {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_extra() {
                    continue;
                }
                if child.kind() == "(" || child.kind() == ")" {
                    continue;
                }
                if child.start_position().row > lparen_row {
                    has_content_on_new_line = true;
                }
                break;
            }
        }
        let suppress_paren_cont = has_content_on_new_line && self.in_binary_rhs;
        let prev_in_binary_rhs = self.in_binary_rhs;
        let paren_opens_line = {
            let pos = node.start_position();
            let line_start = node.start_byte() - pos.column;
            self.src[line_start..node.start_byte()]
                .iter()
                .all(|b| *b == b' ' || *b == b'\t' || *b == b'!')
        };
        let content_is_binary = {
            let mut cursor = node.walk();
            node.children(&mut cursor).any(|c| {
                !c.is_extra() && matches!(c.kind(), "binary_expression" | "ternary_expression")
            })
        };
        if has_content_on_new_line || (paren_opens_line && content_is_binary) {
            self.in_binary_rhs = false;
        }

        let cond_paren = matches!(
            node.parent().map(|p| p.kind()),
            Some("if_statement")
                | Some("while_statement")
                | Some("do_statement")
                | Some("synchronized_statement")
                | Some("switch_expression")
        );

        let mut parts = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_row: Option<usize> = None;
        let mut content_on_new_line = false;
        let mut prev_was_comment = false;
        let mut leading_comment_before_content = false;
        let mut prev_end_byte: Option<usize> = None;
        for child in node.children(&mut cursor) {
            if child.is_extra() {
                let sp = match prev_end_row {
                    None => Spacing::none(),
                    Some(pr) if child.start_position().row > pr => Spacing::new_line(),
                    Some(_) => {
                        if prev_end_byte.is_some_and(|pb| child.start_byte() > pb) {
                            Spacing::one_space()
                        } else {
                            Spacing::none()
                        }
                    }
                };
                let cmt = self.comment_leaf(child).with_spacing(sp);
                let cmt = if cond_paren
                    && !parts.is_empty()
                    && prev_end_row.is_some_and(|pr| child.start_position().row > pr)
                {
                    cmt.with_indent(IndentType::Normal)
                } else if !cond_paren
                    && !suppress_paren_cont
                    && !parts.is_empty()
                    && prev_end_row.is_some_and(|pr| child.start_position().row > pr)
                {
                    cmt.with_indent(IndentType::Continuation)
                } else {
                    cmt
                };
                parts.push(cmt);
                prev_end_row = Some(child.end_position().row);
                prev_end_byte = Some(child.end_byte());
                prev_was_comment = true;
                continue;
            }
            let is_content = child.kind() != "(" && child.kind() != ")";
            if is_content {
                self.paren_direct_child = true;
                leading_comment_before_content |= prev_was_comment;
            }
            let block = self.node(child);
            self.paren_direct_child = false;
            let spacing = if parts.is_empty() {
                Spacing::none()
            } else if child.kind() == ")" {
                let rparen_on_own_line =
                    prev_end_row.is_some_and(|pr| child.start_position().row > pr);
                if rparen_on_own_line {
                    Spacing::new_line()
                } else {
                    Spacing::none()
                }
            } else if child.kind() == "(" {
                Spacing::none()
            } else if parts.last().is_some_and(|b: &Block| b.text == "(") {
                if child.start_position().row > lparen_row {
                    content_on_new_line = true;
                    Spacing::new_line()
                } else {
                    Spacing::none()
                }
            } else if prev_was_comment {
                if prev_end_row.is_some_and(|pr| child.start_position().row > pr) {
                    content_on_new_line = true;
                    Spacing::new_line()
                } else if prev_end_byte.is_some_and(|pb| child.start_byte() > pb) {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                }
            } else {
                Spacing::one_space()
            };
            let block = if !parts.is_empty() {
                block.with_spacing(spacing)
            } else {
                block
            };
            let block = if content_on_new_line
                && !suppress_paren_cont
                && child.kind() != "("
                && child.kind() != ")"
            {
                block.with_indent(IndentType::Continuation)
            } else if child.kind() == ")"
                && suppress_paren_cont
                && !paren_opens_line
                && prev_end_row.is_some_and(|pr| child.start_position().row > pr)
            {
                block.with_indent(IndentType::CancelOneContinuation)
            } else if is_content && cond_paren {
                block.with_break_bonus()
            } else if is_content && (!suppress_paren_cont || !prev_was_comment) {
                block.with_paren_cwf_probe()
            } else {
                block
            };
            let block = if child.kind() == ")"
                && !cond_paren
                && !(suppress_paren_cont
                    && self.cfg.align_multiline_ternary
                    && self.in_ternary_alternative
                    && leading_comment_before_content)
            {
                block.with_paren_close_probe()
            } else {
                block
            };
            parts.push(block);
            prev_end_row = Some(child.end_position().row);
            prev_end_byte = Some(child.end_byte());
            prev_was_comment = false;
        }
        self.in_binary_rhs = prev_in_binary_rhs;
        let block = if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Block::node(parts)
        };
        if suppress_paren_cont
            && self.cfg.align_multiline_ternary
            && self.in_ternary_alternative
            && leading_comment_before_content
        {
            block.with_indent(IndentType::Continuation)
        } else {
            block
        }
    }

    pub(in crate::formatter::build) fn cast_expression(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut after_rparen = false;
        let mut prev_end_row = None;
        let mut before_cast = node.start_byte();
        while before_cast > 0 && matches!(self.src[before_cast - 1], b' ' | b'\t' | b'\r' | b'\n') {
            before_cast -= 1;
        }
        let cast_starts_source_line = self.src[before_cast..node.start_byte()].contains(&b'\n');
        for child in node.children(&mut cursor) {
            if child.is_extra() {
                children.push(self.comment_leaf(child).with_spacing(Spacing::one_space()));
                prev_end_row = Some(child.end_position().row);
                continue;
            }
            let mut block = self.node(child);
            let spacing = if after_rparen {
                after_rparen = false;
                if prev_end_row.is_some_and(|row| child.start_position().row > row) {
                    if cast_starts_source_line {
                        block = block.with_indent(IndentType::Continuation);
                    }
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                }
            } else {
                Spacing::none()
            };
            let block = if !children.is_empty() {
                block.with_spacing(spacing)
            } else {
                block
            };
            if child.kind() == ")" {
                after_rparen = true;
            }
            prev_end_row = Some(child.end_position().row);
            children.push(block);
        }
        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        }
    }
}
