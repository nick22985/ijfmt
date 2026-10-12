use tree_sitter::Node;

use crate::formatter::build::util::wrap_type_from_mode;
use crate::formatter::build::{BlockBuilder, MalformedSupertypeRecovery};
use crate::formatter::ir::*;

fn normalize_varargs(text: &str) -> String {
    if !text.contains("...") {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len() + 4);
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 3 <= bytes.len() && bytes[i] == b'.' && bytes[i + 1] == b'.' && bytes[i + 2] == b'.'
        {
            if !result.is_empty() {
                let last = result.as_bytes()[result.len() - 1];
                if last.is_ascii_alphanumeric() || last == b'_' || last == b')' {
                    result.push(' ');
                }
            }
            result.push_str("...");
            i += 3;
            if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                result.push(' ');
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

impl<'a> BlockBuilder<'a> {
    pub(crate) fn formal_parameters(&mut self, node: Node) -> Block {
        let malformed_signature = node.parent().is_some_and(|parent| {
            (self.malformed_supertype_recovery == Some(MalformedSupertypeRecovery::Generic)
                && matches!(
                    parent.kind(),
                    "method_declaration" | "constructor_declaration"
                ))
                || (self.malformed_supertype_recovery == Some(MalformedSupertypeRecovery::Scoped)
                    && parent.kind() == "method_declaration")
        });
        if malformed_signature {
            return self.malformed_formal_parameters(node);
        }

        let mut cursor = node.walk();
        let all_children: Vec<Node> = node.children(&mut cursor).collect();

        let params: Vec<Node> = all_children
            .iter()
            .filter(|n| n.kind() != "(" && n.kind() != ")" && n.kind() != "," && !n.is_extra())
            .copied()
            .collect();

        let comments: Vec<Node> = all_children
            .iter()
            .filter(|n| n.is_extra())
            .copied()
            .collect();

        if params.is_empty() && !all_children.iter().any(|n| n.kind() == "ERROR") {
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
            let row_gap = rparen_row.saturating_sub(lparen_row);
            let mut parts = vec![Block::leaf("(")];
            for c in &comments {
                let on_own_line = c.start_position().row > lparen_row;
                let spacing = if on_own_line {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                };
                parts.push(self.comment_leaf(*c).with_spacing(spacing));
            }
            let rparen_spacing = if rparen_row > lparen_row {
                if row_gap >= 2 && self.cfg.keep_blank_lines_in_code > 0 {
                    let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1).max(1);
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
            parts.push(Block::leaf(")").with_spacing(rparen_spacing));
            return Block::node(parts);
        }

        let error_params: Vec<Node> = all_children
            .iter()
            .filter(|n| n.kind() == "ERROR" && self.text(**n).trim_start().starts_with(','))
            .copied()
            .collect();
        let recoverable_errors = !error_params.is_empty()
            && !params.is_empty()
            && all_children.iter().filter(|n| n.kind() == "ERROR").count() == error_params.len();
        let (params, comments) = if recoverable_errors {
            let mut p = params;
            p.extend(error_params.iter().copied());
            p.sort_by_key(|n| n.start_byte());
            let c = comments
                .into_iter()
                .filter(|n| n.kind() != "ERROR")
                .collect::<Vec<_>>();
            (p, c)
        } else {
            (params, comments)
        };

        let has_error_children =
            !recoverable_errors && all_children.iter().any(|n| n.kind() == "ERROR");
        if has_error_children {
            let mut block = self.fallback(node);
            fn normalize_varargs_in_tree(block: &mut Block) {
                block.text = normalize_varargs(&block.text);
                for child in &mut block.children {
                    normalize_varargs_in_tree(child);
                }
            }
            normalize_varargs_in_tree(&mut block);
            return block;
        }

        let is_record = node
            .parent()
            .is_some_and(|p| p.kind() == "record_declaration");
        let is_lambda = node
            .parent()
            .is_some_and(|p| p.kind() == "lambda_expression");
        let wrap_mode = if is_record {
            self.cfg.record_components_wrap
        } else {
            self.cfg.method_parameters_wrap
        };
        let wrap_type = if is_lambda {
            WrapType::None
        } else {
            wrap_type_from_mode(wrap_mode)
        };
        let wrap_id = self.ids.wrap_id();
        self.last_params_wrap = if is_record { Some(wrap_id) } else { None };

        let use_alignment = if is_record {
            self.cfg.align_multiline_record_components
        } else {
            self.cfg.align_multiline_parameters
        };
        let alignment_id = if use_alignment {
            Some(self.ids.alignment_id())
        } else {
            None
        };

        let lparen_row = all_children
            .iter()
            .find(|n| n.kind() == "(")
            .map(|n| n.end_position().row)
            .unwrap_or(node.start_position().row);
        let source_multiline = params.iter().any(|p| p.start_position().row > lparen_row);

        let record_collapse_for_implements = false
            && is_record
            && matches!(wrap_type, WrapType::Normal)
            && source_multiline
            && !(params[0].start_position().row > lparen_row)
            && node.parent().is_some_and(|p| {
                let mut c = p.walk();
                p.children(&mut c)
                    .any(|ch| ch.kind() == "super_interfaces" || ch.kind() == "extends_interfaces")
            })
            && {
                let lparen_col = all_children
                    .iter()
                    .find(|n| n.kind() == "(")
                    .map(|n| n.start_position().column)
                    .unwrap_or(node.start_position().column);
                let params_width: usize = params
                    .iter()
                    .map(|p| p.end_byte() - p.start_byte())
                    .sum::<usize>()
                    + 2 * params.len().saturating_sub(1);
                let projected = lparen_col + 1 + params_width + 1 + 11;
                projected + 3 <= self.cfg.right_margin
            };

        let record_collapse_no_implements = false
            && is_record
            && matches!(wrap_type, WrapType::Normal)
            && source_multiline
            && !(params[0].start_position().row > lparen_row)
            && !record_collapse_for_implements
            && !node.parent().is_some_and(|p| {
                let mut c = p.walk();
                p.children(&mut c)
                    .any(|ch| ch.kind() == "super_interfaces" || ch.kind() == "extends_interfaces")
            })
            && {
                let mut breaks = 0;
                for i in 1..params.len() {
                    if params[i].start_position().row > params[i - 1].end_position().row {
                        breaks += 1;
                    }
                }
                breaks == 1
            }
            && {
                let record_node = node.parent();
                let lparen_col = all_children
                    .iter()
                    .find(|n| n.kind() == "(")
                    .map(|n| n.start_position().column)
                    .unwrap_or(node.start_position().column);
                let params_width: usize = params
                    .iter()
                    .map(|p| p.end_byte() - p.start_byte())
                    .sum::<usize>()
                    + 2 * params.len().saturating_sub(1);
                let projected_inline = lparen_col + 1 + params_width + 3;
                let base_col = record_node.map(|r| r.start_position().column).unwrap_or(0);
                let cont_col = base_col + self.cfg.continuation_indent;
                let projected_cont = cont_col + params_width + 3;
                projected_inline > self.cfg.right_margin
                    && projected_cont + 3 <= self.cfg.right_margin
            };

        let method_collapse_inline_fits = false
            && !is_record
            && matches!(wrap_type, WrapType::Normal)
            && source_multiline
            && params
                .iter()
                .all(|p| p.start_position().row == p.end_position().row)
            && {
                let rparen_row = all_children
                    .iter()
                    .find(|n| n.kind() == ")")
                    .map(|n| n.start_position().row)
                    .unwrap_or(lparen_row);
                let last_param_row = params
                    .last()
                    .map(|p| p.end_position().row)
                    .unwrap_or(lparen_row);
                rparen_row == last_param_row
            }
            && {
                let first_start = params[0].start_byte();
                let last_end = params.last().unwrap().end_byte();
                !comments
                    .iter()
                    .any(|c| c.start_byte() >= first_start && c.end_byte() <= last_end)
            }
            && {
                let lparen = all_children.iter().find(|n| n.kind() == "(");
                let src_lparen_col = lparen
                    .map(|n| n.start_position().column)
                    .unwrap_or(node.start_position().column);
                let method_decl = node.parent();
                let out_base_col = method_decl.map(|m| self.compute_output_col(m)).unwrap_or(0);
                let src = self.src;
                let row_start_byte = {
                    let lparen_byte = lparen.map(|n| n.start_byte()).unwrap_or(node.start_byte());
                    let mut b = lparen_byte;
                    while b > 0 && src[b - 1] != b'\n' {
                        b -= 1;
                    }
                    b
                };
                let src_first_col = {
                    let mut c = 0usize;
                    let mut b = row_start_byte;
                    while b < src.len() && (src[b] == b' ' || src[b] == b'\t') {
                        c += 1;
                        b += 1;
                    }
                    c
                };
                let prefix_on_row = src_lparen_col.saturating_sub(src_first_col);
                let out_lparen_col = out_base_col + prefix_on_row;
                let params_width: usize = params
                    .iter()
                    .map(|p| p.end_byte() - p.start_byte())
                    .sum::<usize>()
                    + 2 * params.len().saturating_sub(1);
                let rparen = all_children.iter().find(|n| n.kind() == ")");
                let suffix_len = if let Some(r) = rparen {
                    let rparen_byte = r.start_byte();
                    let nl = src[rparen_byte..]
                        .iter()
                        .position(|&b| b == b'\n')
                        .unwrap_or(src.len() - rparen_byte);
                    let mut end = rparen_byte + nl;
                    while end > rparen_byte {
                        let b = src[end - 1];
                        if b == b' ' || b == b'\t' || b == b'\r' {
                            end -= 1;
                        } else {
                            break;
                        }
                    }
                    end - rparen_byte
                } else {
                    3
                };
                let projected_inline = out_lparen_col + 1 + params_width + suffix_len;
                let src_projected_inline = src_lparen_col + 1 + params_width + suffix_len;
                projected_inline <= self.cfg.right_margin
                    && src_projected_inline > self.cfg.right_margin
            };

        let mut children = Vec::new();

        children.push(Block::leaf("("));

        let lparen_end_byte = all_children
            .iter()
            .find(|n| n.kind() == "(")
            .map(|n| n.end_byte())
            .unwrap_or(node.start_byte());
        let mut had_inline_comment_before_first = false;
        for c in &comments {
            if c.start_byte() >= lparen_end_byte && c.start_byte() < params[0].start_byte() {
                let on_own_line = c.start_position().row > lparen_row;
                let is_line_comment = c.kind() == "line_comment";
                let inline_line_comment = is_line_comment && !on_own_line;
                let spacing = if on_own_line {
                    Spacing::new_line()
                } else if inline_line_comment {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                };
                let block = self.comment_leaf(*c).with_spacing(spacing);
                let block = if let Some(aid) = alignment_id {
                    block.with_alignment(aid)
                } else {
                    block
                };
                let block = if on_own_line {
                    block.with_indent(IndentType::Continuation)
                } else if inline_line_comment {
                    let w = Wrap {
                        id: wrap_id,
                        wrap_type,
                        wrap_first_element: true,
                        parent: None,
                        ignore_parent_wraps: false,
                        cand_ignore_parents: false,
                    };
                    self.wraps.insert(w.id, w.clone());
                    block.with_wrap(w).with_indent(IndentType::Continuation)
                } else {
                    had_inline_comment_before_first = true;
                    block
                };
                children.push(block);
            }
        }

        for (i, param) in params.iter().enumerate() {
            let param_block = if param.kind() == "ERROR" {
                Block::leaf(normalize_varargs(
                    self.text(*param)
                        .trim_start()
                        .trim_start_matches(',')
                        .trim(),
                ))
            } else {
                self.node(*param)
            };

            let chop_all = matches!(wrap_type, WrapType::ChopIfNeeded | WrapType::Always);
            let param_spacing = if source_multiline && i > 0 && chop_all {
                Spacing::new_line()
            } else if source_multiline && i == 0 && chop_all {
                let first_on_new_line = params[0].start_position().row > lparen_row;
                if first_on_new_line {
                    Spacing::new_line()
                } else if had_inline_comment_before_first {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                }
            } else if source_multiline {
                if i == 0 {
                    let first_on_new_line = params[0].start_position().row > lparen_row;
                    if first_on_new_line {
                        Spacing::new_line()
                    } else if had_inline_comment_before_first {
                        Spacing::one_space()
                    } else {
                        Spacing::none()
                    }
                } else if param.start_position().row > params[i - 1].end_position().row {
                    if record_collapse_for_implements
                        || record_collapse_no_implements
                        || method_collapse_inline_fits
                    {
                        Spacing::one_space()
                    } else {
                        let previous_content_row = comments
                            .iter()
                            .filter(|comment| {
                                comment.start_byte() >= params[i - 1].end_byte()
                                    && comment.end_byte() <= param.start_byte()
                            })
                            .map(|comment| comment.end_position().row)
                            .max()
                            .unwrap_or(params[i - 1].end_position().row);
                        let row_gap = param
                            .start_position()
                            .row
                            .saturating_sub(previous_content_row);
                        if row_gap >= 2 {
                            let feeds = row_gap.min(self.cfg.keep_blank_lines_in_declarations + 1);
                            Spacing {
                                min_spaces: 0,
                                max_spaces: 0,
                                min_line_feeds: feeds as u32,
                                keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                                keep_line_breaks: true,
                            }
                        } else {
                            Spacing::new_line()
                        }
                    }
                } else {
                    Spacing::one_space()
                }
            } else if i > 0 || had_inline_comment_before_first {
                Spacing::one_space()
            } else {
                Spacing::none()
            };

            let indent_type = if crate::formatter::build::util::is_glued_lambda_param_list(node) {
                IndentType::LambdaParamGlued
            } else {
                IndentType::Continuation
            };

            if i == 0 {
                let first_on_new_line =
                    source_multiline && params[0].start_position().row > lparen_row;
                let w = Wrap {
                    id: wrap_id,
                    wrap_type,
                    wrap_first_element: first_on_new_line
                        || (wrap_type == WrapType::Normal && !is_lambda),
                    parent: None,
                    ignore_parent_wraps: false,
                    cand_ignore_parents: false,
                };
                self.wraps.insert(w.id, w.clone());

                let mut block = param_block
                    .with_wrap(w)
                    .with_indent(indent_type)
                    .with_spacing(param_spacing);
                if let Some(aid) = alignment_id {
                    block = block.with_alignment(aid);
                }
                children.push(block);
            } else {
                let prev_param = params[i - 1];
                let comma_node = all_children.iter().find(|n| {
                    n.kind() == ","
                        && n.start_byte() >= prev_param.end_byte()
                        && n.start_byte() < param.start_byte()
                });
                let comma_on_new_line = source_multiline
                    && comma_node
                        .map(|n| n.start_position().row > prev_param.end_position().row)
                        .unwrap_or(false);

                if comma_on_new_line {
                    for c in &comments {
                        if c.start_byte() >= prev_param.end_byte()
                            && c.start_byte()
                                < comma_node
                                    .map(|n| n.start_byte())
                                    .unwrap_or(param.start_byte())
                        {
                            let on_own_line =
                                c.start_position().row > prev_param.end_position().row;
                            let spacing = if on_own_line {
                                Spacing::new_line()
                            } else {
                                Spacing::one_space()
                            };
                            let block = self.comment_leaf(*c).with_spacing(spacing);
                            let block = if on_own_line {
                                block.with_indent(indent_type)
                            } else {
                                block
                            };
                            children.push(block);
                        }
                    }

                    children.push(
                        Block::leaf(",")
                            .with_spacing(Spacing::new_line())
                            .with_indent(indent_type),
                    );

                    let comma_end = comma_node
                        .map(|n| n.end_byte())
                        .unwrap_or(prev_param.end_byte());
                    for c in &comments {
                        if c.start_byte() >= comma_end && c.start_byte() < param.start_byte() {
                            let on_own_line = c.start_position().row
                                > comma_node
                                    .map(|n| n.end_position().row)
                                    .unwrap_or(prev_param.end_position().row);
                            let spacing = if on_own_line {
                                Spacing::new_line()
                            } else {
                                Spacing::one_space()
                            };
                            let block = self.comment_leaf(*c).with_spacing(spacing);
                            let block = if on_own_line {
                                block.with_indent(indent_type)
                            } else {
                                block
                            };
                            children.push(block);
                        }
                    }

                    children.push(param_block.with_spacing(Spacing::one_space()));
                } else {
                    let comma_start = comma_node
                        .map(|n| n.start_byte())
                        .unwrap_or(param.start_byte());
                    for c in &comments {
                        if c.start_byte() >= prev_param.end_byte() && c.start_byte() < comma_start {
                            let gap = &self.src[prev_param.end_byte()..c.start_byte()];
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
                            children.push(
                                self.comment_leaf(*c)
                                    .with_spacing(Spacing::spaces(spaces, spaces)),
                            );
                        }
                    }

                    children.push(Block::leaf(",").with_spacing(Spacing::none()));

                    let comma_end = comma_node
                        .map(|n| n.end_byte())
                        .unwrap_or(prev_param.end_byte());
                    for c in &comments {
                        if c.start_byte() >= comma_end.max(prev_param.end_byte())
                            && c.start_byte() < param.start_byte()
                        {
                            let on_own_line =
                                c.start_position().row > prev_param.end_position().row;
                            let spacing = if on_own_line {
                                Spacing::new_line()
                            } else {
                                Spacing::one_space()
                            };
                            let mut block = self
                                .comment_leaf(*c)
                                .with_spacing(spacing)
                                .with_indent(indent_type);
                            if !on_own_line {
                                block = block.with_wrap(Wrap {
                                    id: wrap_id,
                                    wrap_type,
                                    wrap_first_element: false,
                                    parent: None,
                                    ignore_parent_wraps: false,
                                    cand_ignore_parents: false,
                                });
                            }
                            if let Some(aid) = alignment_id {
                                block = block.with_alignment(aid);
                            }
                            children.push(block);
                        }
                    }

                    let w = Wrap {
                        id: wrap_id,
                        wrap_type,
                        wrap_first_element: false,
                        parent: None,
                        ignore_parent_wraps: false,
                        cand_ignore_parents: false,
                    };

                    let mut block = param_block
                        .with_wrap(w)
                        .with_indent(indent_type)
                        .with_spacing(param_spacing);
                    if let Some(aid) = alignment_id {
                        block = block.with_alignment(aid);
                    }
                    children.push(block);
                }
            }
        }

        if let Some(last_param) = params.last() {
            let rparen_byte = all_children
                .iter()
                .find(|n| n.kind() == ")")
                .map(|n| n.start_byte())
                .unwrap_or(node.end_byte());
            for c in &comments {
                if c.start_byte() > last_param.end_byte() && c.start_byte() < rparen_byte {
                    let mut on_own_line = c.start_position().row > last_param.end_position().row;
                    if !on_own_line
                        && c.kind() == "line_comment"
                        && source_multiline
                        && self.cfg.right_margin > 0
                    {
                        let param_col =
                            self.compute_output_col(node) + self.cfg.continuation_indent;
                        let param_len = last_param.end_byte() - last_param.start_byte();
                        let comment_len = c.end_byte() - c.start_byte();
                        let total = param_col + param_len + 1 + comment_len;
                        if total > self.cfg.right_margin {
                            on_own_line = true;
                        }
                    }
                    let spacing = if on_own_line {
                        Spacing::new_line()
                    } else {
                        Spacing::one_space()
                    };
                    let mut block = self.comment_leaf(*c).with_spacing(spacing);
                    if on_own_line {
                        block = block.with_indent(IndentType::Continuation);
                    }
                    children.push(block);
                }
            }
        }

        let rparen_row = all_children
            .iter()
            .find(|n| n.kind() == ")")
            .map(|n| n.start_position().row)
            .unwrap_or(0);
        let rparen_byte_for_gap = all_children
            .iter()
            .find(|n| n.kind() == ")")
            .map(|n| n.start_byte())
            .unwrap_or(node.end_byte());
        let last_param_end_byte = params.last().map(|p| p.end_byte()).unwrap_or(0);
        let last_param_row_base = params
            .last()
            .map(|p| p.end_position().row)
            .unwrap_or(lparen_row);
        let trailing_comment_max_row = comments
            .iter()
            .filter(|c| {
                c.start_byte() > last_param_end_byte && c.start_byte() < rparen_byte_for_gap
            })
            .map(|c| c.end_position().row)
            .max();
        let last_param_row = trailing_comment_max_row
            .map(|r| r.max(last_param_row_base))
            .unwrap_or(last_param_row_base);
        let rparen_on_own_line = rparen_row > last_param_row;
        let rparen_spacing = if rparen_on_own_line {
            let row_gap = rparen_row.saturating_sub(last_param_row);
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
        children.push(Block::leaf(")").with_spacing(rparen_spacing));

        Block::node(children)
    }

    fn malformed_formal_parameters(&mut self, node: Node) -> Block {
        let mut cursor = node.walk();
        let all_children: Vec<Node> = node.children(&mut cursor).collect();
        let mut children = Vec::new();
        let mut prev: Option<Node> = None;
        let mut previous_parameter_primitive = false;

        for child in all_children {
            let primitive_parameter = child.child_by_field_name("type").is_some_and(|ty| {
                matches!(
                    ty.kind(),
                    "boolean_type" | "integral_type" | "floating_point_type"
                )
            });
            let mut block = if matches!(child.kind(), "formal_parameter" | "spread_parameter") {
                let text = self.text(child).replace("... ", "...");
                Block::leaf(text).with_source_offset(child.start_byte())
            } else if child.is_extra() {
                self.comment_leaf(child)
            } else {
                self.node(child)
            };
            fn clear_parameter_probes(block: &mut Block) {
                block.param_scope = false;
                block.param_probe = false;
                for child in &mut block.children {
                    clear_parameter_probes(child);
                }
            }
            clear_parameter_probes(&mut block);

            if let Some(previous) = prev {
                let source_break = child.start_position().row > previous.end_position().row;
                let spacing = if child.kind() == ")" && source_break {
                    Spacing::new_line()
                } else if matches!(child.kind(), ")" | ",") {
                    Spacing::none()
                } else if source_break {
                    Spacing::new_line()
                } else if previous.kind() == "(" {
                    let raw = self.text(child).trim_start();
                    if raw.starts_with("final") || primitive_parameter {
                        Spacing::one_space()
                    } else {
                        Spacing::none()
                    }
                } else if previous.kind() == ","
                    && primitive_parameter
                    && !previous_parameter_primitive
                {
                    Spacing::none()
                } else {
                    Spacing::one_space()
                };
                block = block.with_spacing(spacing);
            }

            children.push(block);
            if matches!(child.kind(), "formal_parameter" | "spread_parameter") {
                previous_parameter_primitive = primitive_parameter;
            }
            prev = Some(child);
        }

        Block::node(children)
    }
}
