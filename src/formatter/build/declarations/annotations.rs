use crate::config::WrapMode;
use crate::formatter::build::util::wrap_type_from_mode;
use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::ir::*;

impl<'a> BlockBuilder<'a> {
    pub(in crate::formatter::build) fn element_value_pair(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();

        let raw_children: Vec<Node> = node
            .children(&mut node.walk())
            .filter(|c| !c.is_extra())
            .collect();
        let value_node = raw_children.last().copied();
        let value_is_string = value_node.is_some_and(|v| v.kind() == "string_literal");
        let break_wrap = if value_is_string {
            Some(self.make_wrap(WrapType::Normal))
        } else {
            None
        };

        for child in node.children(&mut cursor) {
            if child.is_extra() {
                children.push(self.comment_leaf(child).with_spacing(Spacing::one_space()));
                continue;
            }
            let block = self.node(child);
            let source_break_before_value = value_node
                .is_some_and(|value| value.id() == child.id())
                && raw_children.iter().rev().nth(1).is_some_and(|previous| {
                    child.start_position().row > previous.end_position().row
                });
            let spacing = if source_break_before_value {
                Spacing::new_line()
            } else if child.kind() == "=" || children.last().is_some_and(|b: &Block| b.text == "=")
            {
                Spacing::one_space()
            } else {
                Spacing::none()
            };
            let is_value = break_wrap.is_some() && value_node.is_some_and(|v| v.id() == child.id());
            let mut block = if !children.is_empty() {
                block.with_spacing(spacing)
            } else {
                block
            };
            if is_value {
                if let Some(ref w) = break_wrap {
                    block = block
                        .with_wrap(w.clone())
                        .with_indent(IndentType::Continuation);
                }
            } else if value_node
                .is_some_and(|v| v.id() == child.id() && v.kind() == "binary_expression")
                && !children.is_empty()
            {
                block = block.with_gated_break_bonus();
            }
            children.push(block);
        }
        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        }
    }

    pub(in crate::formatter::build) fn annotation_argument_list(&mut self, node: Node) -> Block {
        let mut cursor = node.walk();
        let all_children: Vec<Node> = node.children(&mut cursor).collect();

        let args: Vec<Node> = all_children
            .iter()
            .filter(|n| n.kind() != "(" && n.kind() != ")" && n.kind() != "," && !n.is_extra())
            .copied()
            .collect();

        if args.is_empty() {
            return self.fallback(node);
        }

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

        let first_on_same_line = args[0].start_position().row == lparen_row;
        let arg_indent = if self.cfg.align_multiline_annotation_parameters
            && source_multiline
            && first_on_same_line
        {
            let anno_col = node
                .parent()
                .map(|p| self.source_visual_col(p))
                .unwrap_or_else(|| self.source_visual_col(node));
            let head = self.source_visual_col(args[0]).saturating_sub(anno_col);
            IndentType::Spaces(head)
        } else {
            IndentType::Continuation
        };

        let mut children = Vec::new();
        children.push(Block::leaf("("));

        for (i, arg) in args.iter().enumerate() {
            if arg.kind() == "element_value_array_initializer" {
                let mut ac = arg.walk();
                let arr_children: Vec<Node> = arg.children(&mut ac).collect();
                let arr_elems: Vec<Node> = arr_children
                    .iter()
                    .filter(|n| {
                        n.kind() != "{" && n.kind() != "}" && n.kind() != "," && !n.is_extra()
                    })
                    .copied()
                    .collect();

                let lbrace_row = arr_children
                    .iter()
                    .find(|n| n.kind() == "{")
                    .map(|n| n.end_position().row)
                    .unwrap_or(arg.start_position().row);
                let rbrace_row = arr_children
                    .iter()
                    .find(|n| n.kind() == "}")
                    .map(|n| n.start_position().row)
                    .unwrap_or(arg.end_position().row);
                let arr_multiline = arr_elems
                    .iter()
                    .any(|e| e.start_position().row > lbrace_row)
                    || rbrace_row > lbrace_row;

                let first_on_new_line =
                    i == 0 && source_multiline && arg.start_position().row > lparen_row;
                let brace_spacing = if first_on_new_line {
                    Spacing::new_line()
                } else if i == 0 {
                    Spacing::none()
                } else if source_multiline
                    && arg.start_position().row > args[i - 1].end_position().row
                {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                };
                let brace_block = Block::leaf("{").with_spacing(brace_spacing);
                children.push(if first_on_new_line {
                    brace_block.with_indent(arg_indent)
                } else {
                    brace_block
                });

                let wrap_if_long = !arr_multiline
                    && matches!(
                        self.cfg.array_initializer_wrap,
                        crate::config::WrapMode::WrapIfLong
                    );
                let array_wrap = if wrap_if_long {
                    let mut w = self.make_wrap(crate::formatter::ir::WrapType::Normal);
                    w.wrap_first_element = true;
                    self.wraps.insert(w.id, w.clone());
                    Some(w)
                } else {
                    None
                };

                let arr_align = if self.cfg.align_multiline_array_initializer {
                    Some(self.ids.alignment_id())
                } else {
                    None
                };
                for (j, elem) in arr_elems.iter().enumerate() {
                    if j > 0 {
                        children.push(Block::leaf(",").with_spacing(Spacing::none()));
                    }
                    let elem_spacing = if j == 0 {
                        if arr_multiline && elem.start_position().row > lbrace_row {
                            Spacing::new_line()
                        } else {
                            Spacing::none()
                        }
                    } else if arr_multiline
                        && elem.start_position().row > arr_elems[j - 1].end_position().row
                    {
                        let row_gap = elem
                            .start_position()
                            .row
                            .saturating_sub(arr_elems[j - 1].end_position().row);
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
                        Spacing::one_space()
                    };
                    let mut elem_block = self
                        .node(*elem)
                        .with_indent(IndentType::Continuation)
                        .with_spacing(elem_spacing);
                    if let Some(aid) = arr_align {
                        elem_block = elem_block.with_alignment(aid);
                    }
                    if let Some(ref w) = array_wrap {
                        elem_block = elem_block.with_wrap(w.clone());
                    }
                    children.push(elem_block);
                }

                let has_trailing_comma = arr_children
                    .iter()
                    .rev()
                    .find(|n| n.kind() != "}" && !n.is_extra())
                    .is_some_and(|n| n.kind() == ",");
                if has_trailing_comma {
                    children.push(Block::leaf(",").with_spacing(Spacing::none()));
                }

                let last_elem_end_row = arr_elems
                    .last()
                    .map(|e| e.end_position().row)
                    .unwrap_or(lbrace_row);
                let rbrace_on_own_line = arr_multiline && rbrace_row > last_elem_end_row;
                let rbrace_spacing = if rbrace_on_own_line {
                    Spacing::new_line()
                } else {
                    Spacing::none()
                };
                children.push(Block::leaf("}").with_spacing(rbrace_spacing));
                if i + 1 < args.len() {
                    children.push(Block::leaf(",").with_spacing(Spacing::none()));
                }
                continue;
            }

            if arg.kind() == "element_value_pair" {
                let mut pc = arg.walk();
                let pair_children: Vec<Node> = arg.children(&mut pc).collect();
                let value_child = pair_children.iter().rfind(|n| !n.is_extra());
                if value_child.is_some_and(|v| v.kind() == "element_value_array_initializer") {
                    let array_node = *value_child.unwrap();
                    let pair_prefix: Vec<Node> = pair_children
                        .iter()
                        .filter(|n| n.id() != array_node.id() && !n.is_extra())
                        .copied()
                        .collect();

                    let pair_on_new_line = if i == 0 {
                        source_multiline && args[0].start_position().row > lparen_row
                    } else {
                        source_multiline
                            && arg.start_position().row > args[i - 1].end_position().row
                    };
                    let pair_spacing = if pair_on_new_line {
                        Spacing::new_line()
                    } else if i == 0 {
                        Spacing::none()
                    } else {
                        Spacing::one_space()
                    };

                    let mut prefix_parts = Vec::new();
                    for (pi, pn) in pair_prefix.iter().enumerate() {
                        let b = if pn.kind() == "=" {
                            Block::leaf("=")
                        } else {
                            self.node(*pn)
                        };
                        let sp = if pi == 0 {
                            Spacing::none()
                        } else {
                            Spacing::one_space()
                        };
                        prefix_parts.push(if pi > 0 { b.with_spacing(sp) } else { b });
                    }
                    let prefix_block = if prefix_parts.len() == 1 {
                        prefix_parts.pop().unwrap()
                    } else {
                        Block::node(prefix_parts)
                    };
                    children.push(
                        prefix_block
                            .with_indent(arg_indent)
                            .with_spacing(pair_spacing),
                    );

                    let mut ac = array_node.walk();
                    let arr_children: Vec<Node> = array_node.children(&mut ac).collect();
                    let arr_elems: Vec<Node> = arr_children
                        .iter()
                        .filter(|n| {
                            n.kind() != "{" && n.kind() != "}" && n.kind() != "," && !n.is_extra()
                        })
                        .copied()
                        .collect();

                    let lbrace_row = arr_children
                        .iter()
                        .find(|n| n.kind() == "{")
                        .map(|n| n.end_position().row)
                        .unwrap_or(array_node.start_position().row);
                    let rbrace_row = arr_children
                        .iter()
                        .find(|n| n.kind() == "}")
                        .map(|n| n.start_position().row)
                        .unwrap_or(array_node.end_position().row);
                    let arr_multiline = arr_elems
                        .iter()
                        .any(|e| e.start_position().row > lbrace_row)
                        || rbrace_row > lbrace_row;

                    let deep_array = lbrace_row > lparen_row && pair_on_new_line;

                    let brace_on_new_line = pair_prefix
                        .last()
                        .is_some_and(|n| array_node.start_position().row > n.end_position().row);

                    let mut array_parts = Vec::new();
                    array_parts.push(Block::leaf("{").with_spacing(if brace_on_new_line {
                        Spacing::new_line()
                    } else {
                        Spacing::one_space()
                    }));

                    let elem_indent = IndentType::Continuation;
                    let arr_align = if self.cfg.align_multiline_array_initializer {
                        Some(self.ids.alignment_id())
                    } else {
                        None
                    };

                    let wrap_if_long = !arr_multiline
                        && matches!(
                            self.cfg.array_initializer_wrap,
                            crate::config::WrapMode::WrapIfLong
                        );
                    let array_wrap = if wrap_if_long {
                        let mut w = self.make_wrap(crate::formatter::ir::WrapType::Normal);
                        w.wrap_first_element = true;
                        self.wraps.insert(w.id, w.clone());
                        Some(w)
                    } else {
                        None
                    };

                    for (j, elem) in arr_elems.iter().enumerate() {
                        if j > 0 {
                            array_parts.push(Block::leaf(",").with_spacing(Spacing::none()));
                        }
                        let elem_spacing = if j == 0 {
                            if arr_multiline && elem.start_position().row > lbrace_row {
                                Spacing::new_line()
                            } else {
                                Spacing::none()
                            }
                        } else if arr_multiline
                            && elem.start_position().row > arr_elems[j - 1].end_position().row
                        {
                            Spacing::new_line()
                        } else {
                            Spacing::one_space()
                        };
                        let mut elem_block = self
                            .node(*elem)
                            .with_indent(elem_indent)
                            .with_spacing(elem_spacing);
                        if let Some(aid) = arr_align {
                            elem_block = elem_block.with_alignment(aid);
                        }
                        if let Some(ref w) = array_wrap {
                            elem_block = elem_block.with_wrap(w.clone());
                        }
                        array_parts.push(elem_block);
                    }

                    let has_trailing_comma = arr_children
                        .iter()
                        .rev()
                        .find(|n| n.kind() != "}" && !n.is_extra())
                        .is_some_and(|n| n.kind() == ",");
                    if has_trailing_comma {
                        array_parts.push(Block::leaf(",").with_spacing(Spacing::none()));
                    }

                    let last_elem_end_row = arr_elems
                        .last()
                        .map(|e| e.end_position().row)
                        .unwrap_or(lbrace_row);
                    let rbrace_on_own_line = arr_multiline && rbrace_row > last_elem_end_row;
                    let rbrace_spacing = if rbrace_on_own_line {
                        Spacing::new_line()
                    } else {
                        Spacing::none()
                    };
                    array_parts.push(Block::leaf("}").with_spacing(rbrace_spacing));

                    if deep_array || brace_on_new_line {
                        children
                            .push(Block::node(array_parts).with_indent(IndentType::Continuation));
                    } else {
                        children.extend(array_parts);
                    }
                    if i + 1 < args.len() {
                        children.push(Block::leaf(",").with_spacing(Spacing::none()));
                    }
                    continue;
                }
            }

            let arg_block = self.node(*arg);

            let spacing = if i == 0 {
                let first_on_new_line =
                    source_multiline && args[0].start_position().row > lparen_row;
                if first_on_new_line {
                    Spacing::new_line()
                } else {
                    Spacing::none()
                }
            } else if source_multiline && arg.start_position().row > args[i - 1].end_position().row
            {
                Spacing::new_line()
            } else if i > 0 {
                Spacing::one_space()
            } else {
                Spacing::none()
            };

            children.push(arg_block.with_indent(arg_indent).with_spacing(spacing));

            if i + 1 < args.len() {
                children.push(Block::leaf(",").with_spacing(Spacing::none()));
            }

            let arg_end = arg.end_byte();
            let next_start = if i + 1 < args.len() {
                args[i + 1].start_byte()
            } else {
                all_children
                    .iter()
                    .rev()
                    .find(|n| n.kind() == ")")
                    .map(|n| n.start_byte())
                    .unwrap_or(node.end_byte())
            };
            for c in &all_children {
                if c.is_extra() && c.start_byte() >= arg_end && c.start_byte() < next_start {
                    let on_same_line = c.start_position().row == arg.end_position().row;
                    let comment_spacing = if on_same_line {
                        Spacing::one_space()
                    } else {
                        Spacing::new_line()
                    };
                    let comment_block = self.comment_leaf(*c).with_spacing(comment_spacing);
                    let comment_block = if !on_same_line {
                        comment_block.with_indent(arg_indent)
                    } else {
                        comment_block
                    };
                    children.push(comment_block);
                }
            }
        }

        let rparen_on_own_line =
            source_multiline && rparen_row > args.last().map(|a| a.end_position().row).unwrap_or(0);
        let rparen_spacing = if rparen_on_own_line {
            Spacing::new_line()
        } else {
            Spacing::none()
        };
        let last_arg = args.last().unwrap();
        let last_arg_end = last_arg.end_byte();
        let rparen_byte = all_children
            .iter()
            .find(|n| n.kind() == ")")
            .map(|n| n.start_byte())
            .unwrap_or(node.end_byte());
        let last_arg_has_trailing_comment = all_children.iter().any(|c| {
            c.is_extra()
                && c.start_byte() >= last_arg_end
                && c.start_byte() < rparen_byte
                && c.kind() == "line_comment"
        });
        let has_nested_annotation_argument = args.iter().any(|arg| {
            let mut cursor = arg.walk();
            arg.children(&mut cursor)
                .any(|child| matches!(child.kind(), "annotation" | "marker_annotation"))
        });
        let rparen_block = Block::leaf(")").with_spacing(rparen_spacing);
        let rparen_block = if rparen_on_own_line
            && self.cfg.align_multiline_annotation_parameters
            && (first_on_same_line || last_arg_has_trailing_comment)
            && !has_nested_annotation_argument
        {
            rparen_block.with_indent(arg_indent)
        } else {
            rparen_block
        };
        children.push(rparen_block);

        Block::node(children)
    }
    pub(crate) fn modifiers_and_annotations(&mut self, node: Node) -> Block {
        let wrap_mode = if let Some(parent) = node.parent() {
            match parent.kind() {
                "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration" => self.cfg.class_annotation_wrap,
                "field_declaration" | "constant_declaration" => self.cfg.field_annotation_wrap,
                "method_declaration"
                | "constructor_declaration"
                | "compact_constructor_declaration" => self.cfg.method_annotation_wrap,
                "formal_parameter" | "spread_parameter" => self.cfg.parameter_annotation_wrap,
                _ => WrapMode::DoNotWrap,
            }
        } else {
            WrapMode::DoNotWrap
        };

        let wrap_type = wrap_type_from_mode(wrap_mode);

        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut has_annotations = false;
        let mut in_keywords = false;

        let all_children: Vec<Node> = node.children(&mut cursor).collect();

        for (idx, &child) in all_children.iter().enumerate() {
            let is_annotation = matches!(child.kind(), "marker_annotation" | "annotation");
            let is_keyword_modifier =
                !is_annotation && !child.is_extra() && child.child_count() == 0;

            let block = if child.is_extra() {
                self.comment_leaf(child)
            } else {
                self.node(child)
            };

            let spacing = if let Some(pb) = prev_end_byte {
                if wrap_type == WrapType::None {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    let newline_count = gap.iter().filter(|&&b| b == b'\n').count();
                    if newline_count == 1
                        && is_keyword_modifier
                        && !in_keywords
                        && has_annotations
                        && self.text(child) == "final"
                    {
                        Some(Spacing::one_space())
                    } else if newline_count >= 2 {
                        let blank_lines = (newline_count - 1)
                            .min(self.cfg.keep_blank_lines_in_code)
                            .max(1);
                        Some(Spacing {
                            min_spaces: 0,
                            max_spaces: 0,
                            min_line_feeds: (blank_lines + 1) as u32,
                            keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                            keep_line_breaks: true,
                        })
                    } else if newline_count >= 1 {
                        Some(Spacing::new_line())
                    } else if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                        Some(Spacing::one_space())
                    } else {
                        Some(Spacing::none())
                    }
                } else if is_annotation && in_keywords {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.contains(&b'\n') {
                        Some(Spacing::new_line())
                    } else {
                        Some(Spacing::one_space())
                    }
                } else if is_annotation && has_annotations {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    let source_inline = !gap.contains(&b'\n');
                    if source_inline
                        && child.kind() == "marker_annotation"
                        && self.is_type_use_annotation(child)
                        && wrap_type != WrapType::Always
                    {
                        let next_is_non_annotation = all_children[(idx + 1)..]
                            .iter()
                            .find(|n| !n.is_extra())
                            .map(|n| !matches!(n.kind(), "marker_annotation" | "annotation"))
                            .unwrap_or(true);
                        if next_is_non_annotation {
                            Some(Spacing::one_space())
                        } else {
                            Some(Spacing::new_line())
                        }
                    } else {
                        let newline_count = gap.iter().filter(|&&b| b == b'\n').count();
                        if newline_count >= 2 {
                            let blank_lines = (newline_count - 1)
                                .min(self.cfg.keep_blank_lines_in_code)
                                .max(1);
                            Some(Spacing {
                                min_spaces: 0,
                                max_spaces: 0,
                                min_line_feeds: (blank_lines + 1) as u32,
                                keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                                keep_line_breaks: true,
                            })
                        } else {
                            Some(Spacing::new_line())
                        }
                    }
                } else if is_keyword_modifier && has_annotations && !in_keywords {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    let newline_count = gap.iter().filter(|&&b| b == b'\n').count();
                    if newline_count >= 2 {
                        let blank_lines = (newline_count - 1)
                            .min(self.cfg.keep_blank_lines_in_code)
                            .max(1);
                        Some(Spacing {
                            min_spaces: 0,
                            max_spaces: 0,
                            min_line_feeds: (blank_lines + 1) as u32,
                            keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                            keep_line_breaks: true,
                        })
                    } else {
                        Some(Spacing::new_line())
                    }
                } else if is_keyword_modifier
                    && wrap_type == WrapType::Always
                    && idx > 0
                    && matches!(
                        all_children[idx - 1].kind(),
                        "marker_annotation" | "annotation"
                    )
                {
                    Some(Spacing::new_line())
                } else if is_keyword_modifier {
                    Some(Spacing::one_space())
                } else {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.contains(&b'\n') {
                        Some(Spacing::new_line())
                    } else if child.is_extra() {
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
                        Some(Spacing::spaces(spaces, spaces))
                    } else {
                        Some(Spacing::one_space())
                    }
                }
            } else {
                None
            };

            if is_annotation {
                has_annotations = true;
            }

            let block = match spacing {
                Some(s) => block.with_spacing(s),
                None => block,
            };

            let is_comment_after_annotations = child.is_extra()
                && child.kind() == "line_comment"
                && has_annotations
                && !in_keywords;
            let block = if is_annotation
                && !in_keywords
                && matches!(wrap_type, WrapType::Normal | WrapType::ChopIfNeeded)
                && prev_end_byte.is_some()
            {
                let wrap = self.make_wrap(wrap_type);
                block.with_wrap(wrap)
            } else if is_comment_after_annotations && prev_end_byte.is_some() {
                let w = Wrap {
                    id: self.ids.wrap_id(),
                    wrap_type: WrapType::Normal,
                    wrap_first_element: true,
                    parent: None,
                    ignore_parent_wraps: false,
                    cand_ignore_parents: false,
                };
                self.wraps.insert(w.id, w.clone());
                block.with_wrap(w)
            } else {
                block
            };

            prev_end_byte = Some(child.end_byte());
            if is_keyword_modifier {
                in_keywords = true;
            }
            children.push(block);
        }

        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        }
    }

    pub(crate) fn is_type_use_annotation(&self, node: Node) -> bool {
        let text = self.text(node);
        let name = text.strip_prefix('@').unwrap_or(text);
        let name = name.split('(').next().unwrap_or(name).trim();
        if let Some(dot) = name.rfind('.') {
            return matches!(
                &name[..dot],
                "org.jetbrains.annotations" | "org.jspecify.annotations"
            );
        }
        let simple = name;
        if simple == "Nullable" && self.checker_nullable_declarative {
            return false;
        }
        if !matches!(
            simple,
            "NotNull" | "NonNull" | "Nullable" | "UnknownNullability" | "NotNullByDefault"
        ) {
            return false;
        }
        let src = std::str::from_utf8(self.src).unwrap_or("");
        for pkg in ["org.jetbrains.annotations", "org.jspecify.annotations"] {
            if src.contains(&format!("import {}.{};", pkg, simple))
                || src.contains(&format!("import {}.*;", pkg))
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn is_type_use_annotation_for_align(&self, node: Node) -> bool {
        let text = self.text(node);
        let name = text.strip_prefix('@').unwrap_or(text);
        let simple = name.rsplit('.').next().unwrap_or(name);
        matches!(
            simple,
            "NotNull"
                | "NonNull"
                | "Nonnull"
                | "Nullable"
                | "CheckForNull"
                | "CheckReturnValue"
                | "NullMarked"
                | "NullUnmarked"
        )
    }
}
