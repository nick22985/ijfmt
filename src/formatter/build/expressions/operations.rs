use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::build::util::wrap_type_from_mode;
use crate::formatter::ir::*;

impl<'a> BlockBuilder<'a> {
    pub(crate) fn binary_expression(&mut self, node: Node) -> Block {
        let mut operands: Vec<Node> = Vec::new();
        let mut operators: Vec<String> = Vec::new();
        let mut operator_rows: Vec<usize> = Vec::new();
        let mut operand_trailing_comments: Vec<Vec<Node>> = Vec::new();
        let mut operator_trailing_comments: Vec<Vec<Node>> = Vec::new();
        let mut operator_leading_comments: Vec<Vec<Node>> = Vec::new();
        self.collect_binary_chain(
            node,
            &mut operands,
            &mut operators,
            &mut operator_rows,
            &mut operand_trailing_comments,
            &mut operator_trailing_comments,
            &mut operator_leading_comments,
        );

        if operands.is_empty() {
            return self.fallback(node);
        }

        let is_paren_direct_child = self.paren_direct_child;
        self.paren_direct_child = false;

        let wrap_type = wrap_type_from_mode(self.cfg.binary_operation_wrap);
        let wrap_id = self.ids.wrap_id();
        let sign_on_next = self.cfg.binary_operation_sign_on_next_line;

        let source_multiline = node.start_position().row != node.end_position().row;

        let nested_binary_rhs = node.parent().is_some_and(|p| {
            p.kind() == "binary_expression" && p.child_by_field_name("right") == Some(node)
        });

        let tern_bin = binary_inside_conditional(node);
        let tern_bin_after_question = operands[0].kind() == "string_literal"
            && self.text(operands[0]).chars().count()
                + self.cfg.continuation_indent.saturating_mul(2)
                > self.cfg.right_margin
            && node.parent().is_some_and(|parent| {
                parent.kind() == "ternary_expression"
                    && parent.child_by_field_name("consequence") == Some(node)
                    && parent
                        .child_by_field_name("alternative")
                        .is_some_and(|alternative| alternative.kind() == "binary_expression")
            });

        let mut children = Vec::new();

        let prev_in_binary = self.in_binary_operand;
        let prev_in_binary_rhs = self.in_binary_rhs;
        let prev_binary_rhs_op_row = self.binary_rhs_operator_row;
        let prev_binary_rhs_op_on_new_line = self.binary_rhs_operator_on_new_line;
        let prev_in_broken_ternary_branch = self.in_broken_ternary_branch;
        let parent_sign_at_eol = self.binary_parent_sign_at_eol;
        self.binary_parent_sign_at_eol = false;

        let same_line_as_enclosing_arg = self.arg_continuation_depth == 1
            && self
                .enclosing_arg_lparen_row
                .is_some_and(|row| row == node.start_position().row);
        let binary_inside_broken_ternary = prev_in_broken_ternary_branch;
        let leftmost_in_binary = node
            .parent()
            .filter(|p| p.kind() == "binary_expression")
            .is_some_and(|p| p.start_byte() == node.start_byte());
        let operand_indent = if (prev_in_binary_rhs && !binary_inside_broken_ternary)
            || same_line_as_enclosing_arg
            || leftmost_in_binary
        {
            IndentType::None
        } else {
            IndentType::Continuation
        };
        if std::env::var("IJFMT_BIN_DBG").is_ok() {
            eprintln!(
                "[BIN] node={:?} op0={:?} indent={:?} rhs={} bbt={} sl={} lm={}",
                self.text(node).chars().take(40).collect::<String>(),
                operators.first(),
                operand_indent,
                prev_in_binary_rhs,
                binary_inside_broken_ternary,
                same_line_as_enclosing_arg,
                leftmost_in_binary
            );
        }
        let prev_enc_cont_start_row = self.enclosing_cont_start_row;
        self.in_binary_operand = true;
        self.in_binary_rhs = false;

        let prev_enclosing = self.enclosing_arg_wrap;
        let prev_binary_ctx = self.binary_ctx_wrap;

        let prev_nonbin = self.nonbin_enclosing_wrap;
        let entered_from_binary =
            self.binary_ctx_wrap.is_some() && self.enclosing_arg_wrap == self.binary_ctx_wrap;
        let nonbin_enclosing = if entered_from_binary {
            prev_nonbin
        } else {
            prev_enclosing
        };
        self.nonbin_enclosing_wrap = nonbin_enclosing;

        let first_operand_chain_depth =
            if nested_binary_rhs && operands[0].kind() == "method_invocation" {
                let mut depth = 0usize;
                let mut current = Some(operands[0]);
                while let Some(call) = current {
                    if call.kind() != "method_invocation" {
                        break;
                    }
                    depth += 1;
                    current = call.child_by_field_name("object");
                }
                depth
            } else {
                0
            };
        let mut enclosing_if = node.parent();
        while enclosing_if.is_some_and(|ancestor| ancestor.kind() != "if_statement") {
            enclosing_if = enclosing_if.and_then(|ancestor| ancestor.parent());
        }
        let in_else_if = enclosing_if.is_some_and(|if_statement| {
            if_statement.parent().is_some_and(|parent| {
                parent.kind() == "if_statement"
                    && parent.child_by_field_name("alternative") == Some(if_statement)
            })
        });
        let first_operand_chain_must_escape = first_operand_chain_depth >= 3
            && operands[1].kind() == "field_access"
            && in_else_if
            && node.parent().is_some_and(|parent| {
                parent
                    .child_by_field_name("left")
                    .is_some_and(|left| left.kind() == "binary_expression")
            });

        if operands[0].kind() == "binary_expression" {
            self.enclosing_arg_wrap = Some(wrap_id);
            self.binary_ctx_wrap = Some(wrap_id);
        } else if first_operand_chain_must_escape {
            self.enclosing_arg_wrap = nonbin_enclosing;
        }
        let first_block = self.node(operands[0]);
        self.enclosing_arg_wrap = prev_enclosing;
        self.binary_ctx_wrap = prev_binary_ctx;
        if self.enclosing_cont_start_row.is_none() {
            self.enclosing_cont_start_row = Some(node.start_position().row);
        }
        let w = Wrap {
            id: wrap_id,
            wrap_type,
            wrap_first_element: false,
            parent: self.enclosing_arg_wrap,
            ignore_parent_wraps: false,
            cand_ignore_parents: false,
        };
        self.wraps.insert(w.id, w.clone());
        self.positional_wraps.insert(wrap_id);
        let mut first_block = first_block.with_wrap(w);
        if tern_bin_after_question {
            first_block = first_block.with_tern_bin_probe();
        }
        children.push(if first_operand_chain_must_escape {
            first_block.with_wrap_no_promote()
        } else {
            first_block
        });

        for i in 0..operators.len() {
            let op_text = &operators[i];
            let operand = operands[i + 1];

            let w = Wrap {
                id: wrap_id,
                wrap_type,
                wrap_first_element: false,
                parent: self.enclosing_arg_wrap,
                ignore_parent_wraps: false,
                cand_ignore_parents: false,
            };
            self.wraps.insert(w.id, w.clone());

            let prev_operand_end_row = operands.get(i).map(|n| n.end_position().row).unwrap_or(0);
            let operand_source_row = operand.start_position().row;
            let op_row = operator_rows
                .get(i)
                .copied()
                .unwrap_or(prev_operand_end_row);
            let source_break = source_multiline && operand_source_row > prev_operand_end_row;

            let source_op_at_end_of_line = source_break && op_row == prev_operand_end_row;

            let spacing = if source_break {
                let row_gap = operand_source_row.saturating_sub(prev_operand_end_row);
                let has_comments_around_operator = operand_trailing_comments
                    .get(i)
                    .is_some_and(|comments| !comments.is_empty())
                    || operator_trailing_comments
                        .get(i)
                        .is_some_and(|comments| !comments.is_empty())
                    || operator_leading_comments
                        .get(i)
                        .is_some_and(|comments| !comments.is_empty());
                if row_gap >= 2
                    && self.cfg.keep_blank_lines_in_code > 0
                    && !has_comments_around_operator
                    && operand.kind() == "parenthesized_expression"
                    && operand
                        .named_child(0)
                        .is_some_and(|content| content.kind() == "ternary_expression")
                {
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
            };

            let effective_sign_on_next =
                sign_on_next || (source_break && !source_op_at_end_of_line);
            let operand_coms = operand_trailing_comments
                .get(i)
                .cloned()
                .unwrap_or_default();
            let inline_coms = operator_trailing_comments
                .get(i)
                .cloned()
                .unwrap_or_default();
            let standalone_coms = operator_leading_comments
                .get(i)
                .cloned()
                .unwrap_or_default();

            for c in &operand_coms {
                let is_standalone = c.start_position().row > prev_operand_end_row;
                if is_standalone {
                    children.push(
                        self.comment_leaf(*c)
                            .with_spacing(Spacing::new_line())
                            .with_indent(operand_indent),
                    );
                } else {
                    let prev_end = operands.get(i).map(|n| n.end_byte()).unwrap_or(0);
                    let spaces = if c.start_byte() > prev_end {
                        self.src[prev_end..c.start_byte()]
                            .iter()
                            .filter(|&&b| b == b' ' || b == b'\t')
                            .count() as u32
                    } else {
                        0
                    };
                    children.push(
                        self.comment_leaf(*c)
                            .with_wrap(w.clone())
                            .with_indent(operand_indent)
                            .with_spacing(Spacing::spaces(spaces, spaces)),
                    );
                }
            }

            self.in_binary_rhs = true;
            self.binary_rhs_operator_row = Some(operator_rows.get(i).copied().unwrap_or(0));
            self.binary_rhs_operator_on_new_line = source_break;
            if operand.kind() == "binary_expression"
                || (operand.kind() == "unary_expression"
                    && matches!(op_text.as_str(), "&&" | "||")
                    && operators.len() >= 3
                    && i == operators.len() - 1
                    && {
                        let root_call = operand.named_child(0);
                        let mut call = root_call;
                        let mut chain_depth = 0usize;
                        while let Some(node) = call {
                            if node.kind() != "method_invocation" {
                                break;
                            }
                            chain_depth += 1;
                            call = node.child_by_field_name("object");
                        }
                        let nested_call_argument = root_call.is_some_and(|call| {
                            let mut cursor = call.walk();
                            call.children(&mut cursor)
                                .find(|child| child.kind() == "argument_list")
                                .and_then(|arguments| arguments.named_child(0))
                                .is_some_and(|argument| {
                                    let mut nested = Some(argument);
                                    let mut depth = 0usize;
                                    while let Some(call) = nested {
                                        if call.kind() != "method_invocation" {
                                            break;
                                        }
                                        depth += 1;
                                        nested = call.child_by_field_name("object");
                                    }
                                    depth >= 2
                                })
                        });
                        chain_depth >= 2 && nested_call_argument
                    })
            {
                self.enclosing_arg_wrap = Some(wrap_id);
                self.binary_ctx_wrap = Some(wrap_id);
            } else {
                self.enclosing_arg_wrap = nonbin_enclosing;
            }
            if effective_sign_on_next && !source_op_at_end_of_line {
                let operand_block = self.node(operand);
                let mut group_children = vec![Block::leaf(op_text.clone())];
                for c in &inline_coms {
                    group_children.push(self.comment_leaf(*c).with_spacing(Spacing::one_space()));
                }
                for c in &standalone_coms {
                    group_children.push(self.comment_leaf(*c).with_spacing(Spacing::new_line()));
                }
                let op_operand_break = source_multiline && operand_source_row > op_row;
                let operand_block = if tern_bin {
                    operand_block.with_tern_bin_probe()
                } else {
                    operand_block
                };
                group_children.push(
                    operand_block
                        .with_wrap_if(
                            parent_sign_at_eol
                                && nested_binary_rhs
                                && matches!(op_text.as_str(), "&&" | "||")
                                && operand.kind() == "binary_expression",
                            w.clone(),
                        )
                        .with_bin_probe()
                        .with_bin_child_probe()
                        .with_spacing(if standalone_coms.is_empty() && !op_operand_break {
                            Spacing::one_space()
                        } else {
                            Spacing::new_line()
                        }),
                );
                let mut group = Block::node(group_children)
                    .with_wrap(w)
                    .with_wrap_no_promote()
                    .with_indent(operand_indent)
                    .with_spacing(spacing)
                    .with_bin_probe();
                group = group.with_bin_child_probe();
                if tern_bin {
                    group = group.with_tern_bin_probe();
                }

                children.push(group);
            } else if sign_on_next && source_op_at_end_of_line {
                let mut op_block = Block::leaf(op_text.clone())
                    .with_wrap(w.clone())
                    .with_indent(operand_indent)
                    .with_spacing(Spacing::one_space())
                    .with_bin_probe();
                op_block = op_block.with_bin_child_probe();
                if tern_bin {
                    op_block = op_block.with_tern_bin_probe();
                }
                children.push(op_block);

                for c in &inline_coms {
                    children.push(
                        self.comment_leaf(*c)
                            .with_wrap(w.clone())
                            .with_indent(operand_indent)
                            .with_spacing(Spacing::one_space()),
                    );
                }
                for c in &standalone_coms {
                    children.push(
                        self.comment_leaf(*c)
                            .with_spacing(Spacing::new_line())
                            .with_indent(operand_indent),
                    );
                }

                self.binary_parent_sign_at_eol = true;
                let operand_block = self.node(operand);
                self.binary_parent_sign_at_eol = false;
                let operand_block = if tern_bin {
                    operand_block.with_tern_bin_probe()
                } else {
                    operand_block
                };
                children.push(
                    operand_block
                        .with_indent(operand_indent)
                        .with_spacing(Spacing::new_line())
                        .with_bin_probe()
                        .with_bin_child_probe(),
                );
            } else {
                children.push(Block::leaf(format!(" {}", op_text)).with_spacing(Spacing::none()));

                for c in &inline_coms {
                    children.push(
                        self.comment_leaf(*c)
                            .with_wrap(w.clone())
                            .with_indent(operand_indent)
                            .with_spacing(Spacing::one_space()),
                    );
                }

                for c in &standalone_coms {
                    children.push(
                        self.comment_leaf(*c)
                            .with_spacing(Spacing::new_line())
                            .with_indent(operand_indent),
                    );
                }

                let mut operand_block = self
                    .node(operand)
                    .with_wrap(w)
                    .with_indent(operand_indent)
                    .with_spacing(spacing)
                    .with_bin_probe();
                operand_block = operand_block.with_bin_child_probe();
                if tern_bin {
                    operand_block = operand_block.with_tern_bin_probe();
                }
                children.push(operand_block);
            }
            self.in_binary_rhs = false;
            self.enclosing_arg_wrap = prev_enclosing;
            self.binary_ctx_wrap = prev_binary_ctx;
        }
        self.nonbin_enclosing_wrap = prev_nonbin;

        self.in_binary_operand = prev_in_binary;
        self.in_binary_rhs = prev_in_binary_rhs;
        self.binary_rhs_operator_row = prev_binary_rhs_op_row;
        self.binary_rhs_operator_on_new_line = prev_binary_rhs_op_on_new_line;
        self.in_broken_ternary_branch = prev_in_broken_ternary_branch;
        self.enclosing_cont_start_row = prev_enc_cont_start_row;
        let bin_channel = matches!(operand_indent, IndentType::None)
            && !(prev_in_binary_rhs && !binary_inside_broken_ternary);
        if bin_channel {
            Block::node(children).with_bin_anchor().with_bin_scope()
        } else if matches!(operand_indent, IndentType::None)
            && is_paren_direct_child
            && prev_in_binary_rhs
        {
            Block::node(children)
                .with_bin_paren_anchor()
                .with_bin_scope()
        } else {
            Block::node(children).with_bin_scope()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_binary_chain<'b>(
        &self,
        node: Node<'b>,
        operands: &mut Vec<Node<'b>>,
        operators: &mut Vec<String>,
        operator_rows: &mut Vec<usize>,
        operand_trailing_comments: &mut Vec<Vec<Node<'b>>>,
        operator_trailing_comments: &mut Vec<Vec<Node<'b>>>,
        operator_leading_comments: &mut Vec<Vec<Node<'b>>>,
    ) {
        if node.kind() == "binary_expression" {
            let left = node.child_by_field_name("left");
            let op_node = node.child_by_field_name("operator");
            let right_node = node.child_by_field_name("right");

            let my_op = op_node.map(|o| self.text(o).to_string());
            let should_recurse = left.is_some_and(|l| {
                if l.kind() != "binary_expression" {
                    return false;
                }
                let left_op = l
                    .child_by_field_name("operator")
                    .map(|o| self.text(o).to_string());
                match (&my_op, &left_op) {
                    (Some(a), Some(b)) => a == b,
                    _ => false,
                }
            });

            if let Some(left) = left {
                if should_recurse {
                    self.collect_binary_chain(
                        left,
                        operands,
                        operators,
                        operator_rows,
                        operand_trailing_comments,
                        operator_trailing_comments,
                        operator_leading_comments,
                    );
                } else {
                    operands.push(left);
                }
            }
            if let Some(op) = op_node {
                operators.push(self.text(op).to_string());
                operator_rows.push(op.start_position().row);
            }

            let mut before_op = Vec::new();
            let mut after_op_inline = Vec::new();
            let mut after_op_standalone = Vec::new();
            if let Some(op) = op_node {
                let left_end = left.map(|l| l.end_byte()).unwrap_or(0);
                let right_start = right_node.map(|r| r.start_byte()).unwrap_or(usize::MAX);
                let op_row = op.start_position().row;

                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if !child.is_extra() {
                        continue;
                    }
                    if child.start_byte() >= left_end && child.start_byte() < op.start_byte() {
                        before_op.push(child);
                    } else if child.start_byte() >= op.end_byte()
                        && child.start_byte() < right_start
                    {
                        if child.start_position().row == op_row {
                            after_op_inline.push(child);
                        } else {
                            after_op_standalone.push(child);
                        }
                    }
                }
            }
            operand_trailing_comments.push(before_op);
            operator_trailing_comments.push(after_op_inline);
            operator_leading_comments.push(after_op_standalone);

            if let Some(right) = right_node {
                operands.push(right);
            }
        } else {
            operands.push(node);
        }
    }

    pub(crate) fn assignment_expression(&mut self, node: Node) -> Block {
        let left = node.child_by_field_name("left");
        let right = node.child_by_field_name("right");

        let mut cursor = node.walk();
        let op_text: String = node
            .children(&mut cursor)
            .find(|c| {
                !c.is_named() && c.kind() != "(" && c.kind() != ")" && self.text(*c).contains('=')
            })
            .map(|c| self.text(c).to_string())
            .unwrap_or_else(|| "=".to_string());

        let wrap_type = wrap_type_from_mode(self.cfg.assignment_wrap);

        let mut children = Vec::new();

        if let Some(l) = left {
            children.push(self.node(l));
        }

        children.push(Block::leaf(format!(" {}", op_text)).with_spacing(Spacing::none()));

        if let Some(r) = right {
            let source_break = left.is_some_and(|l| r.start_position().row > l.end_position().row);
            let rhs_spacing = if source_break {
                Spacing::new_line()
            } else {
                Spacing::one_space()
            };
            let chained =
                self.cfg.align_multiline_assignment && r.kind() == "assignment_expression";
            let rhs_indent = if chained {
                IndentType::None
            } else if source_break || wrap_type != WrapType::None {
                IndentType::Continuation
            } else {
                IndentType::None
            };

            let w = {
                let w = Wrap {
                    id: self.ids.wrap_id(),
                    wrap_type,
                    wrap_first_element: true,
                    parent: None,
                    ignore_parent_wraps: false,
                    cand_ignore_parents: false,
                };
                self.wraps.insert(w.id, w.clone());
                w
            };
            let prev_enclosing = self.enclosing_arg_wrap;
            if !source_break && (wrap_type != WrapType::None || self.binary_ctx_wrap.is_none()) {
                self.enclosing_arg_wrap = Some(w.id);
            }
            let rhs_block = self.node(r);
            self.enclosing_arg_wrap = prev_enclosing;

            let rhs_block = if rhs_indent == IndentType::None && !chained {
                rhs_block.with_break_bonus()
            } else {
                rhs_block
            };
            children.push(
                rhs_block
                    .with_wrap(w)
                    .with_indent(rhs_indent)
                    .with_spacing(rhs_spacing),
            );
        }

        Block::node(children)
    }
}

pub(in crate::formatter::build) fn binary_inside_conditional(node: tree_sitter::Node) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "ternary_expression" => {
                return parent
                    .child_by_field_name("condition")
                    .is_none_or(|c| c.id() != cur.id());
            }
            "parenthesized_expression"
            | "binary_expression"
            | "argument_list"
            | "method_invocation"
            | "object_creation_expression"
            | "array_creation_expression"
            | "lambda_expression"
            | "cast_expression"
            | "unary_expression"
            | "instanceof_expression"
            | "array_access"
            | "field_access"
            | "assignment_expression"
            | "array_initializer" => cur = parent,
            _ => return false,
        }
    }
    false
}
