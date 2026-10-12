use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::build::util::wrap_type_from_mode;
use crate::formatter::ir::*;

impl<'a> BlockBuilder<'a> {
    pub(crate) fn method_invocation(&mut self, node: Node) -> Block {
        let obj = node.child_by_field_name("object");
        let is_chain_root = obj
            .map(|o| o.kind() == "method_invocation")
            .unwrap_or(false);

        let is_inner = node
            .parent()
            .filter(|p| p.kind() == "method_invocation")
            .and_then(|p| p.child_by_field_name("object"))
            .map(|p_obj| p_obj.id() == node.id())
            .unwrap_or(false);

        if is_inner {
            return self.method_invocation_single(node);
        }

        if !is_chain_root {
            return self.method_invocation_single(node);
        }

        let mut segments: Vec<ChainSegment> = Vec::new();
        self.collect_chain(node, &mut segments);
        let ternary_array_index_chain = self.in_ternary_alternative && {
            let mut current = node.parent();
            let mut inside_array_index = false;
            while let Some(parent) = current {
                if parent.kind() == "array_access" {
                    inside_array_index = true;
                }
                if parent.kind() == "ternary_expression" {
                    break;
                }
                current = parent.parent();
            }
            inside_array_index
        };

        let wrap_type = wrap_type_from_mode(self.cfg.method_call_chain_wrap);
        let chain_wrap_id = self.ids.wrap_id();
        let source_multiline = node.start_position().row != node.end_position().row;

        let src_trailing_dot: Vec<bool> = segments
            .iter()
            .enumerate()
            .map(|(i, seg)| {
                if i == 0 {
                    return false;
                }
                match seg {
                    ChainSegment::Call {
                        dot_row, name_row, ..
                    } => {
                        source_multiline
                            && segments.get(i - 1).is_some_and(|prev| {
                                let prev_end = match prev {
                                    ChainSegment::Root { node: n, .. } => n.end_position().row,
                                    ChainSegment::Call {
                                        args, type_args, ..
                                    } => args
                                        .map(|a| a.end_position().row)
                                        .or(type_args.map(|t| t.end_position().row))
                                        .unwrap_or(0),
                                };
                                *dot_row == prev_end && *name_row > *dot_row
                            })
                    }
                    _ => false,
                }
            })
            .collect();
        let mut children = Vec::new();
        let mut pending_root_comments: Vec<Block> = Vec::new();

        for (i, seg) in segments.iter().enumerate() {
            let seg_block = match seg {
                ChainSegment::Root {
                    node: n,
                    trailing_comments: root_tc,
                } => {
                    let b = self.node(*n);
                    if !root_tc.is_empty() {
                        children.push(b);
                        for tc in root_tc {
                            let on_own_line = tc.start_position().row > n.end_position().row;
                            let sp = if on_own_line {
                                Spacing::new_line()
                            } else {
                                Spacing::one_space()
                            };
                            let cb = self.comment_leaf(*tc).with_spacing(sp);
                            if on_own_line {
                                pending_root_comments.push(cb);
                            } else {
                                children.push(cb);
                            }
                        }
                        continue;
                    }
                    b
                }
                ChainSegment::Call {
                    dot_method,
                    args,
                    type_args,
                    dot_comments,
                    trailing_comments,
                    ..
                } => {
                    let mut parts = Vec::new();

                    let mut call_text = String::from(".");
                    if let Some(ta) = type_args {
                        call_text.push_str(self.text(*ta));
                    }
                    call_text.push_str(dot_method);

                    let is_trailing_dot = src_trailing_dot[i];

                    if let Some(args_node) = args {
                        if is_trailing_dot {
                            parts.push(Block::leaf("."));
                            for dc in dot_comments {
                                parts.push(self.comment_leaf(*dc).with_spacing(Spacing::none()));
                            }
                            let mut name_text = String::new();
                            if let Some(ta) = type_args {
                                name_text.push_str(self.text(*ta));
                            }
                            name_text.push_str(dot_method);
                            parts.push(Block::leaf(name_text).with_spacing(Spacing::new_line()));
                        } else {
                            parts.push(Block::leaf(call_text));
                        }
                        let is_last_seg_local = i == segments.len() - 1;
                        let array_segment_width = {
                            let segment_start = match seg {
                                ChainSegment::Call { dot_byte, .. } => *dot_byte,
                                ChainSegment::Root { node, .. } => node.start_byte(),
                            };
                            let mut width = 0usize;
                            let mut in_whitespace = false;
                            for &byte in &self.src[segment_start..args_node.end_byte()] {
                                if byte.is_ascii_whitespace() {
                                    if !in_whitespace {
                                        width += 1;
                                        in_whitespace = true;
                                    }
                                } else {
                                    width += 1;
                                    in_whitespace = false;
                                }
                            }
                            width
                        };
                        let array_segment_over = self.compute_output_col(node)
                            + self.cfg.continuation_indent.saturating_mul(2)
                            + array_segment_width
                            > self.cfg.right_margin;
                        let array_element_chain_call = segments.len() >= 3
                            && node
                                .parent()
                                .is_some_and(|p| p.kind() == "array_initializer")
                            && (is_last_seg_local || array_segment_over);
                        let logical_rhs_ancestor = {
                            let mut ancestor = node.parent();
                            let mut found = false;
                            while let Some(parent) = ancestor {
                                if parent.kind() == "binary_expression" {
                                    let logical = parent
                                        .child_by_field_name("operator")
                                        .is_some_and(|operator| {
                                            matches!(self.text(operator).trim(), "&&" | "||")
                                        });
                                    let in_rhs =
                                        parent.child_by_field_name("right").is_some_and(|right| {
                                            right.start_byte() <= node.start_byte()
                                                && right.end_byte() >= node.end_byte()
                                        });
                                    found = logical && in_rhs;
                                    break;
                                }
                                if matches!(
                                    parent.kind(),
                                    "ternary_expression" | "assignment_expression"
                                ) {
                                    break;
                                }
                                ancestor = parent.parent();
                            }
                            found
                        };
                        let overlong_logical_rhs_call = self.in_binary_rhs
                            && logical_rhs_ancestor
                            && segments.len() == 3
                            && is_last_seg_local
                            && args_node.named_child_count() == 1
                            && args_node.named_child(0).is_some_and(|argument| {
                                if argument.kind() != "method_invocation" {
                                    return false;
                                }
                                let mut cursor = argument.walk();
                                argument.children(&mut cursor).any(|child| {
                                    child.kind() == "argument_list"
                                        && child.named_child_count() >= 2
                                })
                            })
                            && self.compute_output_col(node)
                                + self.cfg.continuation_indent.saturating_mul(2)
                                + array_segment_width
                                + 1
                                > self.cfg.right_margin
                            && segments.get(i - 1).is_some_and(|previous| {
                                matches!(previous, ChainSegment::Call { args: Some(_), .. })
                            });
                        let detached_chain_argument =
                            array_element_chain_call || overlong_logical_rhs_call;
                        let arg_parent = if detached_chain_argument {
                            None
                        } else {
                            Some(chain_wrap_id)
                        };
                        let arg_block = self.argument_list_inner(
                            *args_node,
                            arg_parent,
                            detached_chain_argument,
                        );
                        let seg_dot_on_new_line = if i > 0 {
                            match seg {
                                ChainSegment::Call { dot_row, .. } => {
                                    source_multiline
                                        && segments.get(i - 1).is_some_and(|prev| {
                                            let prev_end = match prev {
                                                ChainSegment::Root { node: n, .. } => {
                                                    n.end_position().row
                                                }
                                                ChainSegment::Call {
                                                    args: a,
                                                    type_args: ta,
                                                    ..
                                                } => a
                                                    .map(|x| x.end_position().row)
                                                    .or(ta.map(|x| x.end_position().row))
                                                    .unwrap_or(0),
                                            };
                                            *dot_row > prev_end
                                        })
                                }
                                _ => false,
                            }
                        } else {
                            false
                        };
                        let chain_root_row_early = segments.first().map(|s| match s {
                            ChainSegment::Root { node: n, .. } => n.start_position().row,
                            ChainSegment::Call { dot_row, .. } => *dot_row,
                        });
                        let seg_has_own_cont = !self.in_binary_operand
                            && !(source_multiline
                                && self
                                    .enclosing_cont_start_row
                                    .zip(chain_root_row_early)
                                    .is_some_and(|(pr, cr)| pr == cr));
                        let arg_block = if i > 0 && !seg_dot_on_new_line && seg_has_own_cont {
                            let mut b = arg_block.with_indent(IndentType::CancelOneContinuation);
                            if let Some(ref mut info) = b.arg_list_anchor {
                                info.seg_cancel = true;
                            }
                            b
                        } else {
                            arg_block
                        };
                        parts.push(arg_block.with_spacing(Spacing::none()));
                    } else {
                        if is_trailing_dot {
                            parts.push(Block::leaf("."));
                            for dc in dot_comments {
                                parts.push(self.comment_leaf(*dc).with_spacing(Spacing::none()));
                            }
                            let mut name_text = String::new();
                            if let Some(ta) = type_args {
                                name_text.push_str(self.text(*ta));
                            }
                            name_text.push_str(dot_method);
                            name_text.push_str("()");
                            parts.push(Block::leaf(name_text).with_spacing(Spacing::new_line()));
                        } else {
                            call_text.push_str("()");
                            parts.push(Block::leaf(call_text));
                        }
                    }

                    let mut prev_end = if let Some(a) = args {
                        a.end_byte()
                    } else {
                        node.end_byte()
                    };
                    let prev_end_row = if let Some(a) = args {
                        a.end_position().row
                    } else {
                        node.end_position().row
                    };
                    let mut prev_row = prev_end_row;
                    for tc in trailing_comments {
                        let on_own_line = tc.start_position().row > prev_row;
                        let spacing = if on_own_line {
                            let row_gap = tc.start_position().row - prev_row;
                            let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1);
                            Spacing {
                                min_spaces: 0,
                                max_spaces: 0,
                                min_line_feeds: capped as u32,
                                keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                                keep_line_breaks: true,
                            }
                        } else if tc.start_byte() > prev_end {
                            let gap = &self.src[prev_end..tc.start_byte()];
                            let ts = self.cfg.indent_size.max(1) as u32;
                            let sp = gap
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
                            Spacing::spaces(sp.max(1), sp.max(1))
                        } else {
                            Spacing::spaces(1, 1)
                        };
                        parts.push(self.comment_leaf(*tc).with_spacing(spacing));
                        prev_end = tc.end_byte();
                        prev_row = tc.end_position().row;
                    }

                    if parts.len() == 1 {
                        parts.pop().unwrap()
                    } else {
                        Block::node(parts)
                    }
                }
            };

            let chain_contained_in_wrap = {
                let mut n = node;
                let mut found = false;
                while let Some(p) = n.parent() {
                    match p.kind() {
                        "binary_expression" => {
                            if p.child_by_field_name("left")
                                .is_some_and(|l| l.id() == n.id())
                            {
                                found = true;
                                break;
                            }
                            if n.kind() == "unary_expression"
                                && p.child_by_field_name("operator").is_some_and(|operator| {
                                    matches!(self.text(operator).trim(), "&&" | "||")
                                })
                                && p.child_by_field_name("left").is_some_and(|left| {
                                    left.kind() == "binary_expression"
                                        && left.child_by_field_name("left").is_some_and(|nested| {
                                            nested.kind() == "binary_expression"
                                        })
                                })
                            {
                                found = true;
                                break;
                            }
                            n = p;
                        }
                        "assignment_expression" => {
                            if p.child_by_field_name("right")
                                .is_some_and(|r| r.id() == n.id())
                            {
                                found = true;
                            }
                            break;
                        }
                        "unary_expression" => {
                            n = p;
                        }
                        "ternary_expression" => {
                            if p.child_by_field_name("consequence")
                                .is_some_and(|c| c.id() == n.id())
                            {
                                n = p;
                            } else {
                                found = true;
                                break;
                            }
                        }
                        "argument_list" => {
                            found = true;
                            break;
                        }
                        _ => break,
                    }
                }
                found
            };
            let chain_parent = if self.binary_ctx_wrap.is_some()
                && self.enclosing_arg_wrap == self.binary_ctx_wrap
                && !chain_contained_in_wrap
            {
                None
            } else {
                self.enclosing_arg_wrap
            };
            let seg_wrappable = i >= 2;
            let w = Wrap {
                id: chain_wrap_id,
                wrap_type,
                wrap_first_element: true,
                parent: chain_parent,
                ignore_parent_wraps: false,
                cand_ignore_parents: false,
            };
            self.wraps.insert(w.id, w.clone());

            if i == 0 {
                children.push(seg_block);
            } else {
                let src_break = match seg {
                    ChainSegment::Call { dot_row, .. } => {
                        source_multiline
                            && segments.get(i - 1).is_some_and(|prev| {
                                let prev_end = match prev {
                                    ChainSegment::Root { node: n, .. } => n.end_position().row,
                                    ChainSegment::Call {
                                        args, type_args, ..
                                    } => args
                                        .map(|a| a.end_position().row)
                                        .or(type_args.map(|t| t.end_position().row))
                                        .unwrap_or(0),
                                };
                                *dot_row > prev_end
                            })
                    }
                    _ => false,
                };
                let spacing = if src_break {
                    let row_gap = match seg {
                        ChainSegment::Call { dot_row, .. } => {
                            let prev_end = segments.get(i - 1).map_or(*dot_row, |prev| {
                                let (base_end, coms) = match prev {
                                    ChainSegment::Root {
                                        node: n,
                                        trailing_comments,
                                    } => (n.end_position().row, trailing_comments),
                                    ChainSegment::Call {
                                        args,
                                        type_args,
                                        trailing_comments,
                                        ..
                                    } => (
                                        args.map(|a| a.end_position().row)
                                            .or(type_args.map(|t| t.end_position().row))
                                            .unwrap_or(*dot_row),
                                        trailing_comments,
                                    ),
                                };
                                coms.last()
                                    .map(|c| c.end_position().row)
                                    .unwrap_or(base_end)
                                    .max(base_end)
                            });
                            dot_row.saturating_sub(prev_end)
                        }
                        _ => 1,
                    };
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
                    Spacing::none()
                };
                let chain_root_row = segments.first().map(|s| match s {
                    ChainSegment::Root { node: n, .. } => n.start_position().row,
                    ChainSegment::Call { dot_row, .. } => *dot_row,
                });
                let suppress_for_arg = self
                    .enclosing_arg_lparen_row
                    .zip(chain_root_row)
                    .is_some_and(|(pr, cr)| pr == cr);
                let indent = if self.in_binary_operand || suppress_for_arg {
                    IndentType::None
                } else if self.cfg.indent_chained_calls {
                    IndentType::Continuation
                } else {
                    IndentType::Normal
                };
                let channel = u8::from(self.cfg.indent_chained_calls);
                let seg_block = if seg_wrappable {
                    seg_block.with_wrap(w)
                } else {
                    seg_block
                };
                let seg_block = if ternary_array_index_chain
                    && self.in_binary_operand
                    && self.cfg.indent_chained_calls
                {
                    seg_block.with_break_bonus()
                } else {
                    seg_block
                };
                for cb in pending_root_comments.drain(..) {
                    children.push(cb.with_indent(indent));
                }
                children.push(
                    seg_block
                        .with_indent(indent)
                        .with_spacing(spacing)
                        .with_chain_probe()
                        .with_chain_seg_channel(channel),
                );
            }
        }

        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children).with_chain_anchor()
        }
    }

    fn method_invocation_single(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_kind: Option<String> = None;

        let dot_on_new_line = {
            let mut pc = node.walk();
            let mut seen_object = false;
            let mut prev_end: Option<usize> = None;
            let mut result = false;
            for ch in node.children(&mut pc) {
                if ch.is_extra() {
                    continue;
                }
                if ch.kind() == "." && seen_object {
                    if let Some(pb) = prev_end
                        && ch.start_byte() > pb
                    {
                        let gap = &self.src[pb..ch.start_byte()];
                        if gap.contains(&b'\n') {
                            result = true;
                        }
                    }
                    break;
                }
                prev_end = Some(ch.end_byte());
                seen_object = true;
            }
            result
        };
        let parent_stmt_level = node.parent().is_some_and(|p| {
            matches!(
                p.kind(),
                "variable_declarator" | "assignment_expression" | "return_statement"
            )
        });
        let apply_chain_cont = dot_on_new_line
            && node.child_by_field_name("object").is_some()
            && !self.in_binary_operand
            && (self.enclosing_arg_wrap.is_none() || parent_stmt_level);
        let mark_dot_channel = dot_on_new_line
            && node.child_by_field_name("object").is_some()
            && !self.in_binary_operand
            && !apply_chain_cont;
        let mark_dot_channel_bin = dot_on_new_line
            && node.child_by_field_name("object").is_some()
            && self.in_binary_operand;
        let mut past_dot = false;

        for child in node.children(&mut cursor) {
            if child.is_extra() {
                let block = self.comment_leaf(child);
                let spacing = if child.kind() == "line_comment" {
                    if let Some(pb) = prev_end_byte {
                        if child.start_byte() > pb {
                            let gap = &self.src[pb..child.start_byte()];
                            if !gap.contains(&b'\n') {
                                let ts = self.cfg.indent_size.max(1) as u32;
                                let sp = gap
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
                                Spacing::spaces(sp.max(1), sp.max(1))
                            } else {
                                Spacing::new_line()
                            }
                        } else {
                            Spacing::one_space()
                        }
                    } else {
                        Spacing::one_space()
                    }
                } else {
                    Spacing::one_space()
                };
                children.push(block.with_spacing(spacing));
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            let block = if child.kind() == "argument_list" {
                self.argument_list(child)
            } else {
                self.node(child)
            };

            let (spacing, has_newline_before) = if let Some(pb) = prev_end_byte {
                if child.start_byte() > pb {
                    let gap = &self.src[pb..child.start_byte()];
                    if gap.contains(&b'\n') {
                        (Spacing::new_line(), true)
                    } else if child.kind() == "argument_list"
                        || child.kind() == "type_arguments"
                        || child.kind() == "."
                        || matches!(prev_kind.as_deref(), Some("." | "type_arguments"))
                    {
                        (Spacing::none(), false)
                    } else if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                        (Spacing::one_space(), false)
                    } else {
                        (Spacing::none(), false)
                    }
                } else {
                    (Spacing::none(), false)
                }
            } else {
                (Spacing::none(), false)
            };

            let block = if prev_end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };

            if child.kind() == "." {
                past_dot = true;
            }
            let block = if (apply_chain_cont && past_dot)
                || (has_newline_before && child.kind() == "argument_list")
            {
                block.with_indent(IndentType::Continuation)
            } else {
                block
            };
            let block = if mark_dot_channel && child.kind() == "." {
                block.with_chain_probe().with_chain_seg_channel(1)
            } else if mark_dot_channel_bin && child.kind() == "." {
                block.with_chain_probe().with_chain_seg_channel(3)
            } else {
                block
            };

            prev_end_byte = Some(child.end_byte());
            prev_kind = Some(child.kind().to_string());
            children.push(block);
        }

        if children.len() == 1 {
            children.pop().unwrap()
        } else if mark_dot_channel || mark_dot_channel_bin {
            Block::node(children).with_chain_anchor()
        } else {
            Block::node(children)
        }
    }

    fn collect_chain<'b>(&self, node: Node<'b>, segments: &mut Vec<ChainSegment<'b>>) {
        if node.kind() != "method_invocation" {
            segments.push(ChainSegment::Root {
                node,
                trailing_comments: Vec::new(),
            });
            return;
        }

        let obj = node.child_by_field_name("object");
        let name = node.child_by_field_name("name");
        let args = node.child_by_field_name("arguments");
        let type_args = node.child_by_field_name("type_arguments");

        if let Some(o) = obj {
            self.collect_chain(o, segments);
        } else {
            segments.push(ChainSegment::Root {
                node,
                trailing_comments: Vec::new(),
            });
            return;
        }

        let (dot_row, dot_start_byte) = {
            let mut c = node.walk();
            let dot = node.children(&mut c).find(|ch| ch.kind() == ".");
            (
                dot.map(|ch| ch.start_position().row)
                    .unwrap_or(node.start_position().row),
                dot.map(|ch| ch.start_byte()).unwrap_or(node.start_byte()),
            )
        };

        {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.is_extra()
                    && child.start_byte() < dot_start_byte
                    && let Some(last) = segments.last_mut()
                {
                    match last {
                        ChainSegment::Root {
                            trailing_comments, ..
                        }
                        | ChainSegment::Call {
                            trailing_comments, ..
                        } => {
                            trailing_comments.push(child);
                        }
                    }
                }
            }
        }
        let method_name = name.map(|n| self.text(n).to_string()).unwrap_or_default();

        let mut dot_comments = Vec::new();
        {
            let name_start = name.map(|n| n.start_byte()).unwrap_or(node.end_byte());
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.is_extra()
                    && child.start_byte() >= dot_start_byte
                    && child.start_byte() < name_start
                {
                    dot_comments.push(child);
                }
            }
        }

        let last_byte = args
            .map(|a| a.end_byte())
            .or_else(|| name.map(|n| n.end_byte()))
            .unwrap_or(node.end_byte());
        let mut trailing_comments = Vec::new();
        {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.is_extra() && child.start_byte() >= last_byte {
                    trailing_comments.push(child);
                }
            }
        }

        let name_row = name.map(|n| n.start_position().row).unwrap_or(dot_row);
        segments.push(ChainSegment::Call {
            dot_method: method_name,
            args,
            type_args,
            dot_row,
            dot_byte: dot_start_byte,
            name_row,
            dot_comments,
            trailing_comments,
        });
    }
}

enum ChainSegment<'a> {
    Root {
        node: Node<'a>,
        trailing_comments: Vec<Node<'a>>,
    },
    Call {
        dot_method: String,
        args: Option<Node<'a>>,
        type_args: Option<Node<'a>>,
        dot_row: usize,
        dot_byte: usize,
        name_row: usize,
        dot_comments: Vec<Node<'a>>,
        trailing_comments: Vec<Node<'a>>,
    },
}
