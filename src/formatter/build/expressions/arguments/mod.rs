mod comments;
mod context;
mod layout;
mod spacing;

use comments::ArgumentPlacement;
use layout::ArgumentIndentation;
use spacing::ArgumentSpacing;
use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::build::util::wrap_type_from_mode;
use crate::formatter::ir::*;

struct ArgumentNodes<'tree> {
    node: Node<'tree>,
    all_children: Vec<Node<'tree>>,
    args: Vec<Node<'tree>>,
    comments: Vec<Node<'tree>>,
}

impl<'tree> ArgumentNodes<'tree> {
    fn new(node: Node<'tree>) -> Self {
        let mut cursor = node.walk();
        let all_children: Vec<Node> = node.children(&mut cursor).collect();

        let args: Vec<Node> = all_children
            .iter()
            .filter(|n| n.kind() != "(" && n.kind() != ")" && n.kind() != "," && !n.is_extra())
            .copied()
            .collect();

        let comments: Vec<Node> = all_children
            .iter()
            .filter(|n| n.is_extra())
            .copied()
            .collect();

        Self {
            node,
            all_children,
            args,
            comments,
        }
    }
}

impl<'a> BlockBuilder<'a> {
    pub(crate) fn argument_list(&mut self, node: Node) -> Block {
        self.argument_list_inner(node, None, false)
    }

    pub(crate) fn record_pattern_body(&mut self, node: Node) -> Block {
        fn has_missing_descendant(node: Node) -> bool {
            if node.is_missing() {
                return true;
            }
            let mut cursor = node.walk();
            node.children(&mut cursor).any(has_missing_descendant)
        }
        let malformed = has_missing_descendant(node);
        let mut cursor = node.walk();
        let all_children: Vec<Node> = node.children(&mut cursor).collect();

        let components: Vec<Node> = all_children
            .iter()
            .filter(|n| n.kind() != "(" && n.kind() != ")" && n.kind() != "," && !n.is_extra())
            .copied()
            .collect();

        if components.is_empty() || all_children.iter().any(|n| n.is_extra()) {
            return self.fallback(node);
        }

        let wrap_id = self.ids.wrap_id();
        let mk_wrap = |first: bool| Wrap {
            id: wrap_id,
            wrap_type: WrapType::ChopIfNeeded,
            wrap_first_element: first,
            parent: None,
            ignore_parent_wraps: false,
            cand_ignore_parents: false,
        };

        let mut children = vec![Block::leaf("(")];

        for (i, comp) in components.iter().enumerate() {
            if i > 0 {
                children.push(Block::leaf(",").with_spacing(Spacing::none()));
            }
            let w = if i == 0 {
                mk_wrap(true)
            } else {
                let mut wrap = self.make_wrap(WrapType::Normal);
                wrap.parent = Some(wrap_id);
                self.wraps.insert(wrap.id, wrap.clone());
                self.positional_wraps.insert(wrap.id);
                wrap
            };
            if i == 0 {
                self.wraps.insert(w.id, w.clone());
            }
            let spacing = if i == 0 {
                Spacing::none()
            } else {
                Spacing::one_space()
            };
            children.push(
                self.node(*comp)
                    .with_wrap(w)
                    .with_indent(IndentType::Continuation)
                    .with_spacing(spacing),
            );
        }

        children.push(
            Block::leaf(")")
                .with_wrap(mk_wrap(false))
                .with_indent(if malformed {
                    IndentType::Continuation
                } else {
                    IndentType::None
                })
                .with_spacing(Spacing::none()),
        );

        Block::node(children)
    }

    pub(in crate::formatter::build) fn argument_list_inner(
        &mut self,
        node: Node,
        explicit_parent: Option<WrapId>,
        ignore_parent_wraps: bool,
    ) -> Block {
        let parent_wrap = explicit_parent.or(self.enclosing_arg_wrap);
        let ignore_nested_binary_parents = ignore_parent_wraps
            || (explicit_parent.is_none()
                && self.binary_ctx_wrap.is_some()
                && self.enclosing_arg_wrap == self.binary_ctx_wrap
                && self.nonbin_enclosing_wrap != self.enclosing_arg_wrap
                && node
                    .named_child(0)
                    .is_some_and(|arg| self.text(arg).trim_start().starts_with("((")));

        let source = ArgumentNodes::new(node);
        let args = &source.args;
        let all_children = &source.all_children;
        if args.is_empty() {
            return self.fallback(node);
        }

        let wrap_type = wrap_type_from_mode(self.cfg.call_parameters_wrap);
        let wrap_id = self.ids.wrap_id();

        let lparen_row = all_children
            .iter()
            .find(|n| n.kind() == "(")
            .map(|n| n.end_position().row)
            .unwrap_or(node.start_position().row);
        let rparen_row = all_children
            .iter()
            .find(|n| n.kind() == ")")
            .map(|n| n.start_position().row)
            .unwrap_or(node.end_position().row);
        let source_multiline =
            args.iter().any(|a| a.start_position().row > lparen_row) || rparen_row > lparen_row;

        let ArgumentIndentation {
            same_line_as_outer,
            suppress_for_binary_rhs,
            in_indented_chain_override,
            same_line_as_outer_for_indent,
            nested_first_call_in_wrapped_object_creation,
            chain_line_capped,
            arg_indent,
            first_on_new_line_early,
            chain_channel_eligible,
            in_indented_chain_call,
        } = self.argument_indentation(node, args, lparen_row, rparen_row, source_multiline);

        let mut children = Vec::new();
        children.push(Block::leaf("("));

        let leading_comments =
            self.leading_argument_comments(&source, lparen_row, arg_indent, &mut children);

        let (arg_indent, single_arg_large_inner) = self.with_argument_context(|builder| {
            builder.enclosing_arg_wrap = Some(wrap_id);
            builder.in_binary_operand = false;
            builder.in_binary_rhs = false;
            builder.binary_rhs_operator_row = None;
            builder.binary_rhs_operator_on_new_line = false;
            builder.in_broken_ternary_branch = false;
            if source_multiline {
                builder.enclosing_cont_start_row = Some(lparen_row);
            }
            builder.enclosing_arg_lparen_row = Some(lparen_row);
            if matches!(arg_indent, IndentType::Continuation) {
                builder.arg_continuation_depth += 1;
            }
            builder.in_single_arg_list = args.len() == 1;
            let outer_break_before_arg = source_multiline && {
                let mut has = first_on_new_line_early;
                if !has {
                    for i in 1..args.len() {
                        if args[i].start_position().row > args[i - 1].end_position().row {
                            has = true;
                            break;
                        }
                    }
                }
                has
            };
            builder.enclosing_arg_has_break = outer_break_before_arg;

            let effective_parent_wrap = if same_line_as_outer {
                None
            } else {
                parent_wrap
            };
            if same_line_as_outer {
                builder.in_same_line_outer_arg = true;
            }

            let lone_lambda_arg = args.len() == 1
                && args[0].kind() == "lambda_expression"
                && args[0].end_position().row > args[0].start_position().row;

            let first_on_new_line = source_multiline && args[0].start_position().row > lparen_row;
            let single_arg_large_inner =
                builder.single_arg_large_inner(node, args, all_children, source_multiline);

            let single_arg_nested_call_lambda_body = args.len() == 1
                && args[0].kind() == "method_invocation"
                && args[0]
                    .child_by_field_name("object")
                    .is_none_or(|object| object.kind() != "method_invocation")
                && node.parent().is_some_and(|call| {
                    call.parent().is_some_and(|lambda| {
                        lambda.kind() == "lambda_expression"
                            && lambda
                                .child_by_field_name("body")
                                .is_some_and(|body| body.id() == call.id())
                    })
                });
            let arg_indent = if single_arg_large_inner || single_arg_nested_call_lambda_body {
                IndentType::None
            } else {
                arg_indent
            };

            let break_before_last_for_nested_indent_overflow = builder
                .break_before_last_for_nested_indent_overflow(
                    node,
                    args,
                    all_children,
                    source_multiline,
                    wrap_type,
                );

            let spacing = if source_multiline {
                ArgumentSpacing::Multiline {
                    first_predecessor: first_on_new_line
                        .then_some(leading_comments.predecessor_row),
                    chop_all: matches!(wrap_type, WrapType::ChopIfNeeded | WrapType::Always),
                    break_before_last: break_before_last_for_nested_indent_overflow,
                    keep_blank_lines: builder.cfg.keep_blank_lines_in_code,
                }
            } else {
                ArgumentSpacing::Inline {
                    space_before_first: leading_comments.inline_space,
                }
            };

            for (i, arg) in args.iter().enumerate() {
                let arg_block = builder.node(*arg);

                let arg_spacing = spacing.before(&source, i);

                let placement = if i == 0 {
                    ArgumentPlacement::Wrapped
                } else {
                    builder.argument_separator(
                        &source,
                        i,
                        source_multiline,
                        arg_indent,
                        &mut children,
                    )
                };
                let arg_block = match placement {
                    ArgumentPlacement::AfterOwnLineComma => {
                        arg_block.with_spacing(Spacing::one_space())
                    }
                    ArgumentPlacement::Wrapped => {
                        let wrap = Wrap {
                            id: wrap_id,
                            wrap_type,
                            wrap_first_element: first_on_new_line,
                            parent: effective_parent_wrap,
                            ignore_parent_wraps: ignore_nested_binary_parents,
                            cand_ignore_parents: lone_lambda_arg,
                        };
                        builder.wraps.insert(wrap.id, wrap.clone());
                        if i == 0 {
                            builder.positional_wraps.insert(wrap_id);
                        }
                        arg_block
                            .with_wrap(wrap)
                            .with_indent(arg_indent)
                            .with_spacing(arg_spacing)
                    }
                };
                children.push(arg_block.with_arg_probe());
            }

            (arg_indent, single_arg_large_inner)
        });

        let last_content_row = self.trailing_argument_comments(&source, arg_indent, &mut children);

        let rparen_on_own_line = source_multiline && rparen_row > last_content_row;
        let rparen_spacing = if rparen_on_own_line {
            let row_gap = rparen_row.saturating_sub(last_content_row);
            if row_gap >= 2 && self.cfg.keep_blank_lines_before_rbrace > 0 {
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
            } else {
                Spacing::new_line()
            }
        } else {
            Spacing::none()
        };
        let direct_switch_rule_call = node.parent().is_some_and(|call| {
            call.kind() == "method_invocation"
                && call.parent().is_some_and(|parent| {
                    parent.kind() == "switch_rule"
                        || (parent.kind() == "expression_statement"
                            && parent
                                .parent()
                                .is_some_and(|rule| rule.kind() == "switch_rule"))
                })
        });
        let rparen_indent = if rparen_on_own_line
            && (same_line_as_outer || suppress_for_binary_rhs || direct_switch_rule_call)
        {
            IndentType::CancelOneContinuation
        } else if rparen_on_own_line && in_indented_chain_call {
            IndentType::Continuation
        } else {
            IndentType::None
        };
        children.push(
            Block::leaf(")")
                .with_spacing(rparen_spacing)
                .with_indent(rparen_indent)
                .with_anchor_exempt(),
        );

        let elem_has_cont = matches!(arg_indent, IndentType::Continuation);
        let anchor_channel = same_line_as_outer_for_indent
            && !nested_first_call_in_wrapped_object_creation
            && !in_indented_chain_override
            && !suppress_for_binary_rhs;

        let apply_chain_wrapper =
            in_indented_chain_call && !in_indented_chain_override && children.len() >= 3;
        let anchor_info = ArgAnchorInfo {
            elem_has_cont,
            channel: anchor_channel,
            chain_channel: chain_channel_eligible && !in_indented_chain_override,
            chain_wrapper_static: apply_chain_wrapper,
            chain_line_capped,
            seg_cancel: false,
            enum_const: node.parent().is_some_and(|p| p.kind() == "enum_constant"),
            bin_suppressed: suppress_for_binary_rhs,
            smart: node.parent().is_some_and(|p| {
                matches!(
                    p.kind(),
                    "method_invocation" | "explicit_constructor_invocation"
                )
            }),
            ignore_parent_wraps,
            collapsed: single_arg_large_inner,
        };
        if apply_chain_wrapper {
            let lparen = children.remove(0);
            let rparen = children.pop().unwrap();
            let inner = Block::node(children).with_indent(IndentType::Continuation);
            return Block::node(vec![lparen, inner, rparen]).with_arg_list_anchor(anchor_info);
        }

        Block::node(children).with_arg_list_anchor(anchor_info)
    }
}
