use super::*;

pub(super) struct ArgumentIndentation {
    pub(super) same_line_as_outer: bool,
    pub(super) suppress_for_binary_rhs: bool,
    pub(super) in_indented_chain_override: bool,
    pub(super) same_line_as_outer_for_indent: bool,
    pub(super) nested_first_call_in_wrapped_object_creation: bool,
    pub(super) chain_line_capped: bool,
    pub(super) arg_indent: IndentType,
    pub(super) first_on_new_line_early: bool,
    pub(super) chain_channel_eligible: bool,
    pub(super) in_indented_chain_call: bool,
}

impl BlockBuilder<'_> {
    pub(super) fn argument_indentation(
        &self,
        node: Node,
        args: &[Node],
        lparen_row: usize,
        rparen_row: usize,
        source_multiline: bool,
    ) -> ArgumentIndentation {
        let prev_paren_row_early = self.enclosing_cont_start_row;
        let same_line_as_outer = source_multiline && (prev_paren_row_early == Some(lparen_row));
        let first_arg_on_new_line_check = source_multiline
            && args
                .first()
                .is_some_and(|a| a.start_position().row > lparen_row);
        let suppress_for_binary_rhs = self.in_binary_rhs
            && !(self.binary_rhs_operator_on_new_line && !first_arg_on_new_line_check)
            && self
                .binary_rhs_operator_row
                .is_none_or(|op_row| op_row == lparen_row);
        let broken_outer_call = self.first_broken_outer_call(node);
        let in_indented_chain_override = suppress_for_binary_rhs && broken_outer_call.is_some();
        let suppress_for_binary_rhs = suppress_for_binary_rhs && !in_indented_chain_override;
        let same_line_as_outer_for_indent = self.enclosing_arg_lparen_row == Some(lparen_row);
        let nested_first_call_in_wrapped_object_creation = args.len() == 1
            && node.parent().is_some_and(|call| {
                call.kind() == "method_invocation"
                    && call.parent().is_some_and(|constructor_args| {
                        constructor_args.kind() == "argument_list"
                            && constructor_args
                                .named_child(0)
                                .is_some_and(|first| first.id() == call.id())
                            && constructor_args.named_child_count() >= 2
                            && constructor_args.named_child(1).is_some_and(|next| {
                                next.start_position().row > call.end_position().row
                            })
                            && constructor_args.parent().is_some_and(|constructor| {
                                constructor.kind() == "object_creation_expression"
                                    && constructor.parent().is_some_and(|outer_args| {
                                        outer_args.kind() == "argument_list"
                                            && outer_args.named_child_count() == 1
                                            && outer_args.parent().is_some_and(|outer_call| {
                                                outer_call.kind() == "method_invocation"
                                            })
                                    })
                            })
                    })
            });
        let suppress_same_line = same_line_as_outer_for_indent
            && (!self.enclosing_arg_has_break || nested_first_call_in_wrapped_object_creation)
            && !in_indented_chain_override;
        let broken_chain_in_array_element = node.parent().is_some_and(|call| {
            if call.kind() != "method_invocation" {
                return false;
            }
            if !self.call_dot_starts_line(call) {
                return false;
            }
            let mut root = call;
            while let Some(parent) = root.parent() {
                if parent.kind() == "method_invocation"
                    && parent
                        .child_by_field_name("object")
                        .is_some_and(|object| object.id() == root.id())
                {
                    root = parent;
                    continue;
                }
                break;
            }
            root.parent()
                .is_some_and(|parent| parent.kind() == "array_initializer")
        });
        let starts_on_chain_line = source_multiline
            && node.parent().is_some_and(|call| {
                call.kind() == "method_invocation"
                    && call.child_by_field_name("object").is_none()
                    && call
                        .parent()
                        .is_some_and(|parent| parent.kind() == "argument_list")
            })
            && {
                let line_start = self.src[..node.start_byte()]
                    .iter()
                    .rposition(|byte| *byte == b'\n')
                    .map_or(0, |pos| pos + 1);
                let prefix = &self.src[line_start..node.start_byte()];
                prefix.contains(&b',')
                    && prefix
                        .iter()
                        .copied()
                        .find(|byte| !byte.is_ascii_whitespace())
                        == Some(b'.')
            };
        let chain_line_capped = broken_chain_in_array_element || starts_on_chain_line;
        let arg_indent = if suppress_same_line || suppress_for_binary_rhs || chain_line_capped {
            IndentType::None
        } else {
            IndentType::Continuation
        };

        let first_on_new_line_early = source_multiline
            && args
                .first()
                .is_some_and(|a| a.start_position().row > lparen_row);
        let outer_arg_break = source_multiline && {
            let mut has_break = first_on_new_line_early;
            if !has_break {
                for i in 1..args.len() {
                    if args[i].start_position().row > args[i - 1].end_position().row {
                        has_break = true;
                        break;
                    }
                }
            }
            if !has_break
                && let Some(last) = args.last()
                && rparen_row > last.end_position().row
            {
                has_break = true;
            }
            has_break
        };
        let chain_channel_eligible = node.parent().is_some_and(|mi| {
            if mi.kind() != "method_invocation" {
                return false;
            }
            if self.call_dot_starts_line(mi) {
                return false;
            }
            mi.parent().is_some_and(|outer| {
                outer.kind() == "method_invocation"
                    && outer
                        .child_by_field_name("object")
                        .is_some_and(|obj| obj.id() == mi.id())
            })
        });
        let in_indented_chain_call = outer_arg_break
            && matches!(arg_indent, IndentType::Continuation)
            && broken_outer_call.is_some_and(|outer| {
                in_indented_chain_override
                    || self.enclosing_cont_start_row != Some(outer.start_position().row)
            });

        ArgumentIndentation {
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
        }
    }
    fn call_dot_starts_line(&self, call: Node) -> bool {
        call.child_by_field_name("object").is_some_and(|object| {
            let name_start = call
                .child_by_field_name("name")
                .map_or(call.start_byte(), |name| name.start_byte());
            name_start > object.end_byte()
                && self.src[object.end_byte()..name_start].contains(&b'\n')
        })
    }

    fn first_broken_outer_call<'tree>(&self, arguments: Node<'tree>) -> Option<Node<'tree>> {
        let mut call = arguments.parent()?;
        if call.kind() != "method_invocation" || self.call_dot_starts_line(call) {
            return None;
        }
        while let Some(outer) = call.parent() {
            if outer.kind() != "method_invocation"
                || !outer
                    .child_by_field_name("object")
                    .is_some_and(|object| object.id() == call.id())
            {
                break;
            }
            if self.call_dot_starts_line(outer) {
                return Some(outer);
            }
            call = outer;
        }
        None
    }

    pub(super) fn single_arg_large_inner(
        &self,
        node: Node,
        args: &[Node],
        all_children: &[Node],
        source_multiline: bool,
    ) -> bool {
        args.len() == 1
            && !source_multiline
            && matches!(
                args[0].kind(),
                "method_invocation" | "object_creation_expression"
            )
            && {
                let inner = args[0];
                let mut c = inner.walk();
                let has_inner_arglist_with_commas = inner.children(&mut c).any(|ch| {
                    if ch.kind() == "argument_list" {
                        let mut cc = ch.walk();
                        ch.children(&mut cc).filter(|n| n.kind() == ",").count() >= 1
                    } else {
                        false
                    }
                });
                let inner_len = inner.end_byte().saturating_sub(inner.start_byte());
                let margin = self.cfg.right_margin;
                let threshold = margin.saturating_sub(self.cfg.continuation_indent * 2 + 3);
                let fits_at_cont_col = {
                    if let Some(outer_call) = node.parent() {
                        let lparen_row = node.start_position().row;
                        let outer_start_row = outer_call.start_position().row;
                        if lparen_row == outer_start_row {
                            let base_col = self.compute_output_col(outer_call);
                            let cont_col = base_col + self.cfg.continuation_indent;
                            let suffix_len = if let Some(rparen) =
                                all_children.iter().find(|n| n.kind() == ")")
                            {
                                let rparen_end = rparen.end_byte();
                                let mut line_end = rparen_end;
                                while line_end < self.src.len() && self.src[line_end] != b'\n' {
                                    line_end += 1;
                                }
                                let suffix = &self.src[rparen_end..line_end];
                                std::str::from_utf8(suffix)
                                    .map(|s| s.chars().count())
                                    .unwrap_or(suffix.len())
                            } else {
                                0
                            };
                            cont_col + inner_len + suffix_len < margin
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                has_inner_arglist_with_commas && inner_len > threshold && !fits_at_cont_col
            }
    }
    pub(super) fn break_before_last_for_nested_indent_overflow(
        &self,
        node: Node,
        args: &[Node],
        all_children: &[Node],
        source_multiline: bool,
        wrap_type: WrapType,
    ) -> bool {
        source_multiline
            && matches!(wrap_type, WrapType::Normal)
            && self.arg_continuation_depth > 1
            && args.len() >= 3
            && args.last().is_some_and(|arg| {
                matches!(
                    arg.kind(),
                    "identifier"
                        | "field_access"
                        | "decimal_integer_literal"
                        | "decimal_floating_point_literal"
                        | "true"
                        | "false"
                        | "null_literal"
                )
            })
            && {
                let last = args.len() - 1;
                let row = args[last].start_position().row;
                let row_start = args
                    .iter()
                    .position(|arg| arg.start_position().row == row)
                    .unwrap_or(last);
                let same_source_line = row_start < last
                    && args[row_start..=last].iter().all(|arg| {
                        arg.start_position().row == row && arg.end_position().row == row
                    });
                let line_start = args[last]
                    .start_byte()
                    .saturating_sub(args[last].start_position().column);
                let mut line_end = args[last].end_byte();
                while line_end < self.src.len() && self.src[line_end] != b'\n' {
                    line_end += 1;
                }
                let source_line_width =
                    self.src[line_start..line_end]
                        .iter()
                        .fold(0usize, |column, byte| {
                            if *byte == b'\t' {
                                (column / self.cfg.indent_size.max(1) + 1)
                                    * self.cfg.indent_size.max(1)
                            } else {
                                column + 1
                            }
                        });
                if !same_source_line || source_line_width <= self.cfg.right_margin {
                    false
                } else {
                    let output_indent = self.compute_output_col(node)
                        + self.arg_continuation_depth as usize * self.cfg.continuation_indent;
                    let width = |arg: &Node| self.text(*arg).chars().count();
                    let packed_width = args[row_start..=last].iter().map(width).sum::<usize>()
                        + 2 * (last - row_start);
                    let prefix_width = args[row_start..last].iter().map(width).sum::<usize>()
                        + 2 * (last - row_start - 1)
                        + 1;
                    let suffix_width = all_children
                        .iter()
                        .find(|child| child.kind() == ")")
                        .map_or(1, |rparen| {
                            let mut line_end = rparen.end_byte();
                            while line_end < self.src.len() && self.src[line_end] != b'\n' {
                                line_end += 1;
                            }
                            1 + self.src[rparen.end_byte()..line_end]
                                .iter()
                                .filter(|byte| !byte.is_ascii_whitespace())
                                .count()
                        });
                    output_indent + prefix_width <= self.cfg.right_margin
                        && output_indent + packed_width + suffix_width > self.cfg.right_margin
                }
            }
    }
}
