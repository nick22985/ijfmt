use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::ir::*;
impl<'a> BlockBuilder<'a> {
    pub(crate) fn lambda_expression(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_end_row: Option<usize> = None;
        let mut after_arrow = false;
        let lambda_in_arg_list = node.parent().is_some_and(|p| {
            p.kind() == "argument_list" && p.start_position().row == node.start_position().row
        });

        for child in node.children(&mut cursor) {
            let kind = child.kind();

            if child.is_extra() {
                let block = self.comment_leaf(child);
                let on_new_line = prev_end_row.is_some_and(|r| child.start_position().row > r);
                let spacing = if prev_end_byte.is_none() {
                    Spacing::none()
                } else if on_new_line {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                };
                let mut block = block.with_spacing(spacing);
                if on_new_line && after_arrow && !lambda_in_arg_list {
                    block = block.with_indent(IndentType::Continuation);
                }
                children.push(block);
                prev_end_byte = Some(child.end_byte());
                prev_end_row = Some(child.end_position().row);
                continue;
            }

            if kind == "->" {
                if prev_end_row.is_some_and(|r| child.start_position().row > r) {
                    let arrow = Block::leaf("->").with_spacing(Spacing::new_line());
                    children.push(if lambda_in_arg_list {
                        arrow
                    } else {
                        arrow.with_indent(IndentType::Continuation)
                    });
                    prev_end_byte = Some(child.end_byte());
                    prev_end_row = Some(child.end_position().row);
                    after_arrow = true;
                    continue;
                }
                children.push(Block::leaf(" ->").with_spacing(Spacing::none()));
                prev_end_byte = Some(child.end_byte());
                prev_end_row = Some(child.end_position().row);
                after_arrow = true;
                continue;
            }

            let block = if child == node.child_by_field_name("body").unwrap_or(child)
                && kind == "block"
            {
                let in_multiline_args_no_chain = {
                    let mut p = node.parent();
                    let mut found_arglist = false;
                    let mut in_chain = false;
                    while let Some(parent) = p {
                        if !found_arglist
                            && (parent.kind() == "argument_list"
                                || parent.kind() == "formal_parameters")
                        {
                            let open_paren_row = parent.start_position().row;
                            let lambda_row = node.start_position().row;
                            if lambda_row > open_paren_row {
                                found_arglist = true;
                            } else {
                                break;
                            }
                        }
                        if found_arglist {
                            if parent.kind() == "method_invocation" {
                                if let Some(obj) = parent.child_by_field_name("object")
                                    && matches!(obj.kind(), "method_invocation" | "field_access")
                                {
                                    in_chain = true;
                                }
                                break;
                            }
                            if parent.kind() == "object_creation_expression" {
                                if let Some(gp) = parent.parent()
                                    && gp.kind() == "method_invocation"
                                    && let Some(obj) = gp.child_by_field_name("object")
                                    && obj.id() == parent.id()
                                {
                                    in_chain = true;
                                }
                                break;
                            }
                        }
                        if matches!(
                            parent.kind(),
                            "expression_statement"
                                | "local_variable_declaration"
                                | "return_statement"
                                | "field_declaration"
                                | "method_declaration"
                        ) {
                            break;
                        }
                        p = parent.parent();
                    }
                    found_arglist && !in_chain
                };
                let in_indented_chain = if !in_multiline_args_no_chain {
                    let mut p = node.parent();
                    let mut found = false;
                    while let Some(parent) = p {
                        if parent.kind() == "method_invocation" {
                            let mut current = parent;
                            loop {
                                if let Some(obj) = current.child_by_field_name("object")
                                    && matches!(obj.kind(), "method_invocation" | "field_access")
                                {
                                    let obj_end = obj.end_byte();
                                    let name_start = current
                                        .child_by_field_name("name")
                                        .map(|n| n.start_byte())
                                        .unwrap_or(current.start_byte());
                                    if name_start > obj_end {
                                        let gap = &self.src[obj_end..name_start];
                                        if gap.contains(&b'\n') {
                                            found = true;
                                            break;
                                        }
                                    }
                                    current = obj;
                                    continue;
                                }
                                break;
                            }
                            break;
                        }
                        if matches!(
                            parent.kind(),
                            "expression_statement"
                                | "local_variable_declaration"
                                | "return_statement"
                                | "field_declaration"
                                | "method_declaration"
                        ) {
                            break;
                        }
                        p = parent.parent();
                    }
                    found
                } else {
                    false
                };

                let lambda_at_line_start = {
                    let start = node.start_position();
                    let line_start_byte = node.start_byte() - start.column;
                    let prefix = &self.src[line_start_byte..node.start_byte()];
                    prefix.iter().all(|&b| b == b' ' || b == b'\t')
                };

                let indent = if lambda_at_line_start {
                    IndentType::None
                } else if in_multiline_args_no_chain || in_indented_chain {
                    IndentType::BlockResetKeepOne
                } else {
                    IndentType::BlockReset
                };
                let saved_arg_depth = self.arg_continuation_depth;
                self.arg_continuation_depth = 0;
                let blk = self
                    .code_block(child)
                    .with_indent(indent)
                    .with_anchor_body();
                self.arg_continuation_depth = saved_arg_depth;
                blk
            } else {
                self.node(child)
            };

            let spacing = if let Some(pb) = prev_end_byte {
                if child.start_position().row > prev_end_row.unwrap_or(0) {
                    Spacing::new_line()
                } else if child.start_byte() > pb || after_arrow {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                }
            } else {
                Spacing::none()
            };
            after_arrow = false;

            let is_expr_body = node
                .child_by_field_name("body")
                .is_some_and(|b| b.id() == child.id() && kind != "block");
            let is_body_on_new_line = child.start_position().row > prev_end_row.unwrap_or(0);
            let in_arg_list = node.parent().is_some_and(|p| {
                p.kind() == "argument_list" && p.start_position().row == node.start_position().row
            });

            let block = if prev_end_byte.is_some() {
                let block = block.with_spacing(spacing);
                if is_expr_body && is_body_on_new_line && !in_arg_list {
                    block.with_indent(IndentType::Continuation)
                } else if is_expr_body && is_body_on_new_line && kind == "method_invocation" {
                    block.with_lambda_body_cont()
                } else if is_expr_body && !in_arg_list {
                    block.with_break_bonus()
                } else {
                    block
                }
            } else {
                block
            };

            prev_end_byte = Some(child.end_byte());
            prev_end_row = Some(child.end_position().row);
            children.push(block);
        }

        let result = if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        };
        result.with_anchor_start()
    }

    pub(crate) fn object_creation(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;

        for child in node.children(&mut cursor) {
            if child.is_extra() {
                children.push(self.comment_leaf(child).with_spacing(Spacing::one_space()));
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            let block = match child.kind() {
                "argument_list" => self.argument_list(child),
                "class_body" => {
                    let new_at_line_start = {
                        let start = node.start_position();
                        let line_start_byte = node.start_byte() - start.column;
                        let prefix = &self.src[line_start_byte..node.start_byte()];
                        prefix.iter().all(|&b| b == b' ' || b == b'\t')
                    };
                    let in_multiline_chain = if !new_at_line_start {
                        let mut p = node.parent();
                        let mut found = false;
                        while let Some(parent) = p {
                            if parent.kind() == "method_invocation" {
                                if let Some(obj) = parent.child_by_field_name("object")
                                    && matches!(obj.kind(), "method_invocation" | "field_access")
                                {
                                    let obj_start_row = obj.start_position().row;
                                    let name_row = parent
                                        .child_by_field_name("name")
                                        .map(|n| n.start_position().row)
                                        .unwrap_or(parent.start_position().row);
                                    if name_row > obj_start_row {
                                        found = true;
                                    }
                                }
                                break;
                            }
                            if matches!(
                                parent.kind(),
                                "expression_statement"
                                    | "local_variable_declaration"
                                    | "return_statement"
                                    | "field_declaration"
                                    | "method_declaration"
                            ) {
                                break;
                            }
                            p = parent.parent();
                        }
                        found
                    } else {
                        false
                    };
                    let indent = if new_at_line_start {
                        IndentType::None
                    } else if in_multiline_chain {
                        IndentType::BlockResetKeepOne
                    } else {
                        IndentType::BlockReset
                    };
                    self.class_body(child)
                        .with_indent(indent)
                        .with_anchor_body()
                }
                _ => self.node(child),
            };

            let mut kept_break = false;
            let spacing = if let Some(pb) = prev_end_byte {
                if child.kind() == "argument_list" || child.kind() == "type_arguments" {
                    Spacing::none()
                } else if child.kind() == "class_body" {
                    use crate::config::BraceStyle;
                    match self.cfg.class_brace_style {
                        BraceStyle::NextLine => self.lbrace_new_line(child),
                        BraceStyle::EndOfLine => Spacing::one_space(),
                    }
                } else {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.contains(&b'\n') {
                        kept_break = true;
                        Spacing::new_line()
                    } else if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                        Spacing::one_space()
                    } else {
                        Spacing::none()
                    }
                }
            } else {
                Spacing::none()
            };

            let block = if prev_end_byte.is_some() {
                let block = if kept_break {
                    Block::node(vec![block.with_indent(IndentType::Continuation)])
                        .with_anchor_body()
                } else {
                    block
                };
                block.with_spacing(spacing)
            } else {
                block
            };

            prev_end_byte = Some(child.end_byte());
            children.push(block);
        }

        let result = if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        };
        result.with_anchor_start()
    }

    pub(crate) fn array_creation(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_end_row: Option<usize> = None;

        for child in node.children(&mut cursor) {
            if child.is_extra() {
                children.push(self.comment_leaf(child).with_spacing(Spacing::one_space()));
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            let block = match child.kind() {
                "array_initializer" => self.array_initializer(child),
                "dimensions_expr" => self.node(child),
                _ => self.node(child),
            };

            let initializer_source_break = child.kind() == "array_initializer"
                && prev_end_row.is_some_and(|row| child.start_position().row > row);
            let spacing = if prev_end_byte.is_none() {
                Spacing::none()
            } else if initializer_source_break {
                Spacing::new_line()
            } else if child.kind() == "array_initializer" {
                if self.cfg.space_before_array_initializer_lbrace {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                }
            } else if child.kind() == "dimensions" || child.kind() == "dimensions_expr" {
                Spacing::none()
            } else {
                Spacing::one_space()
            };

            let block = if prev_end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };
            let block = if initializer_source_break {
                block.with_indent(IndentType::Continuation)
            } else {
                block
            };

            prev_end_byte = Some(child.end_byte());
            prev_end_row = Some(child.end_position().row);
            children.push(block);
        }

        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        }
    }

    pub(crate) fn array_initializer(&mut self, node: Node) -> Block {
        let mut cursor = node.walk();
        let all_children: Vec<Node> = node.children(&mut cursor).collect();

        let elems: Vec<Node> = all_children
            .iter()
            .filter(|n| n.kind() != "{" && n.kind() != "}" && n.kind() != ",")
            .copied()
            .collect();

        let open_row = all_children
            .iter()
            .find(|n| n.kind() == "{")
            .map(|n| n.start_position().row)
            .unwrap_or(node.start_position().row);
        let close_row = all_children
            .iter()
            .rev()
            .find(|n| n.kind() == "}")
            .map(|n| n.start_position().row)
            .unwrap_or(node.end_position().row);
        let multiline = close_row > open_row;

        let mut children = Vec::new();
        children.push(Block::leaf("{"));

        if multiline {
            let elem_indent = IndentType::Continuation;

            let force_each_line = matches!(
                self.cfg.array_initializer_wrap,
                crate::config::WrapMode::WrapAlways | crate::config::WrapMode::ChopIfLong
            );

            let ml_wrap = if matches!(
                self.cfg.array_initializer_wrap,
                crate::config::WrapMode::WrapIfLong
            ) {
                let mut w = self.make_wrap(WrapType::Normal);
                w.parent = self.enclosing_arg_wrap;
                self.wraps.insert(w.id, w.clone());
                self.positional_wraps.insert(w.id);
                Some(w)
            } else {
                None
            };

            let arr_align = if self.cfg.align_multiline_array_initializer {
                Some(self.ids.alignment_id())
            } else {
                None
            };

            let real_elems: Vec<&Node> = elems.iter().filter(|e| !e.is_extra()).collect();
            let mut prev_elem_end_row: Option<usize> = None;

            for (i, elem) in elems.iter().enumerate() {
                let elem_row = elem.start_position().row;

                let on_same_source_line = prev_elem_end_row
                    .map(|r| elem_row == r)
                    .unwrap_or(elem_row == open_row);
                let prev_was_block_comment = i > 0
                    && elems[i - 1].is_extra()
                    && elems[i - 1].kind() == "block_comment"
                    && !elem.is_extra();
                let join_after_block_comment = prev_was_block_comment
                    && elems[i - 1].end_position().row == elem.start_position().row
                    && {
                        let comment = elems[i - 1];
                        let comment_len = comment.end_byte() - comment.start_byte();
                        let elem_first_line_end = {
                            let start = elem.start_byte();
                            let end = elem.end_byte();
                            let text = &self.src[start..end];
                            text.iter().position(|&b| b == b'\n').unwrap_or(text.len())
                        };
                        let indent_width =
                            self.compute_output_col(node) + self.cfg.continuation_indent;
                        indent_width + comment_len + 2 + elem_first_line_end
                            <= self.cfg.right_margin
                    };
                let start_new_line = if join_after_block_comment {
                    false
                } else {
                    !on_same_source_line || force_each_line
                };

                let spacing = if join_after_block_comment {
                    Spacing {
                        min_spaces: 2,
                        max_spaces: 2,
                        min_line_feeds: 0,
                        keep_line_breaks: false,
                        keep_blank_lines: 0,
                    }
                } else if start_new_line {
                    let blank_count = if let Some(prev_end) = prev_elem_end_row {
                        let gap = elem_row.saturating_sub(prev_end + 1);
                        gap.min(self.cfg.keep_blank_lines_in_code)
                    } else {
                        0
                    };
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: (1 + blank_count) as u32,
                        keep_line_breaks: false,
                        keep_blank_lines: 0,
                    }
                } else if i == 0 {
                    if elem.is_extra() {
                        Spacing::one_space()
                    } else {
                        Spacing::none()
                    }
                } else if elem.is_extra() {
                    let prev_tok_end = all_children
                        .iter()
                        .filter(|n| n.end_byte() <= elem.start_byte())
                        .map(|n| n.end_byte())
                        .max()
                        .unwrap_or(elem.start_byte());
                    let gap = &self.src[prev_tok_end..elem.start_byte()];
                    let spaces = gap
                        .iter()
                        .filter(|&&b| b == b' ' || b == b'\t')
                        .count()
                        .max(1) as u32;
                    Spacing::spaces(spaces, spaces)
                } else {
                    Spacing::one_space()
                };

                let is_comment = elem.is_extra();

                if !is_comment {
                    let is_last_real = !elems[i + 1..].iter().any(|e| !e.is_extra());

                    let prev_encl = self.enclosing_arg_wrap;
                    if let Some(ref w) = ml_wrap {
                        self.enclosing_arg_wrap = Some(w.id);
                    }
                    let block = self.node(*elem);
                    self.enclosing_arg_wrap = prev_encl;
                    let mut block = block.with_indent(elem_indent).with_spacing(spacing);
                    if let Some(ref w) = ml_wrap {
                        block = block.with_wrap(w.clone());
                    }
                    if let Some(aid) = arr_align {
                        block = block.with_alignment(aid);
                    }
                    children.push(block.with_arr_probe());

                    if !is_last_real {
                        children.push(Block::leaf(",").with_spacing(Spacing::none()));
                    } else {
                        let has_trailing_comma = all_children
                            .iter()
                            .rfind(|n| n.kind() == ",")
                            .map(|c| c.start_byte() >= elem.end_byte())
                            .unwrap_or(false);
                        if has_trailing_comma {
                            children.push(Block::leaf(",").with_spacing(Spacing::none()));
                        }
                    }
                } else {
                    let mut block = self
                        .comment_leaf(*elem)
                        .with_indent(elem_indent)
                        .with_spacing(spacing);
                    if let Some(ref w) = ml_wrap {
                        block = block.with_wrap(w.clone());
                    }
                    if let Some(aid) = arr_align {
                        block = block.with_alignment(aid);
                    }
                    children.push(block.with_arr_probe());
                }

                prev_elem_end_row = Some(elem.end_position().row);
            }

            if real_elems.is_empty() {
                let gap = close_row.saturating_sub(open_row + 1);
                let blanks = gap
                    .min(self.cfg.keep_blank_lines_in_code)
                    .min(self.cfg.keep_blank_lines_before_rbrace);
                children.push(
                    Block::leaf("}")
                        .with_spacing(Spacing {
                            min_spaces: 0,
                            max_spaces: 0,
                            min_line_feeds: (1 + blanks) as u32,
                            keep_line_breaks: false,
                            keep_blank_lines: 0,
                        })
                        .with_arr_close(),
                );
            } else {
                let last_real = real_elems.last().unwrap();
                let has_trailing_comma = all_children
                    .iter()
                    .rfind(|n| n.kind() == ",")
                    .map(|c| c.start_byte() >= last_real.end_byte())
                    .unwrap_or(false);
                let has_comment_after_last = elems
                    .iter()
                    .any(|e| e.is_extra() && e.start_byte() > last_real.end_byte());
                let keep_at_continuation = self.cfg.align_multiline_array_initializer
                    && has_trailing_comma
                    && !has_comment_after_last;
                let source_close_inline = close_row == last_real.end_position().row;
                let close_inline = source_close_inline && !has_comment_after_last;
                if close_inline {
                    children.push(Block::leaf("}").with_spacing(Spacing::none()));
                } else {
                    let last_end_row = elems
                        .iter()
                        .map(|e| e.end_position().row)
                        .max()
                        .unwrap_or(open_row);
                    let blanks = close_row
                        .saturating_sub(last_end_row + 1)
                        .min(self.cfg.keep_blank_lines_before_rbrace);
                    let close_block = Block::leaf("}").with_spacing(Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: (1 + blanks) as u32,
                        keep_line_breaks: false,
                        keep_blank_lines: 0,
                    });
                    if keep_at_continuation {
                        let mut close_block = close_block.with_indent(elem_indent);
                        if let Some(aid) = arr_align {
                            close_block = close_block.with_alignment(aid);
                        }
                        children.push(close_block.with_arr_probe());
                    } else {
                        children.push(close_block.with_arr_close());
                    }
                }
            }
        } else {
            let wrap_if_long = matches!(
                self.cfg.array_initializer_wrap,
                crate::config::WrapMode::WrapIfLong
            );
            let array_wrap = if wrap_if_long {
                let mut w = self.make_wrap(WrapType::Normal);
                w.parent = self.enclosing_arg_wrap;
                self.wraps.insert(w.id, w.clone());
                self.positional_wraps.insert(w.id);
                Some(w)
            } else {
                None
            };

            let real_elems: Vec<&Node> = elems.iter().filter(|e| !e.is_extra()).collect();
            let mut real_idx = 0usize;
            for elem in elems.iter() {
                if elem.is_extra() {
                    let mut block = self.comment_leaf(*elem).with_spacing(Spacing::one_space());
                    if let Some(ref w) = array_wrap {
                        block = block.with_wrap(w.clone());
                        if real_idx > 0 {
                            block = block.with_indent(IndentType::Continuation).with_arr_probe();
                        } else {
                            block = block.with_break_bonus();
                        }
                    }
                    children.push(block);
                } else {
                    let prev_encl = self.enclosing_arg_wrap;
                    if let Some(ref w) = array_wrap {
                        self.enclosing_arg_wrap = Some(w.id);
                    }
                    let mut block = self.node(*elem);
                    self.enclosing_arg_wrap = prev_encl;
                    let spacing = if real_idx == 0 {
                        Spacing::none()
                    } else {
                        Spacing::one_space()
                    };
                    if let Some(ref w) = array_wrap {
                        block = block.with_wrap(w.clone());
                        if real_idx > 0 {
                            block = block.with_indent(IndentType::Continuation);
                        } else {
                            block = block.with_break_bonus();
                        }
                    }
                    let mut block = block.with_spacing(spacing);
                    if array_wrap.is_some() && real_idx > 0 {
                        block = block.with_arr_probe();
                    }
                    children.push(block);
                    if real_idx + 1 < real_elems.len() {
                        children.push(Block::leaf(",").with_spacing(Spacing::none()));
                    }
                    real_idx += 1;
                }
            }
            children.push(Block::leaf("}").with_spacing(Spacing::none()));
        }

        let mut out = Block::node(children).with_arr_scope();
        if self.in_binary_rhs
            && !self.binary_rhs_operator_on_new_line
            && self.binary_rhs_operator_row.is_some_and(|r| r == open_row)
        {
            out = out.with_indent(IndentType::CancelOneContinuation);
        }
        out
    }
}
