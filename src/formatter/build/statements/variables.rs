use tree_sitter::Node;

use crate::config::WrapMode;
use crate::formatter::build::BlockBuilder;
use crate::formatter::ir::*;

impl<'a> BlockBuilder<'a> {
    pub(crate) fn simple_statement(&mut self, node: Node) -> Block {
        self.fallback(node)
    }

    pub(crate) fn declaration_statement(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_end_row: Option<usize> = None;
        let is_class_level = matches!(node.kind(), "field_declaration" | "constant_declaration");
        let is_interface_field = node
            .parent()
            .is_some_and(|parent| parent.kind() == "interface_body");
        let mut prev_was_modifiers = false;
        let mut had_trailing_line_comment_after_modifiers = false;
        let mut prev_was_annotated_modifiers = false;

        let field_anno_wrap = if is_class_level {
            self.cfg.field_annotation_wrap
        } else {
            WrapMode::DoNotWrap
        };

        for child in node.children(&mut cursor) {
            if child.is_extra() {
                if is_class_level
                    && !is_interface_field
                    && prev_was_modifiers
                    && child.kind() == "line_comment"
                {
                    had_trailing_line_comment_after_modifiers = true;
                }
                let spacing = if prev_end_row == Some(child.start_position().row) {
                    let pb = prev_end_byte.unwrap_or(0);
                    let gap_end = child.start_byte().min(self.src.len());
                    let spaces = if pb < gap_end {
                        self.src[pb..gap_end]
                            .iter()
                            .filter(|&&b| b == b' ' || b == b'\t')
                            .count() as u32
                    } else {
                        1
                    };
                    Spacing::spaces(spaces, spaces)
                } else {
                    Spacing::one_space()
                };
                children.push(self.comment_leaf(child).with_spacing(spacing));
                prev_end_byte = Some(child.end_byte());
                prev_end_row = Some(child.end_position().row);
                prev_was_modifiers = false;
                continue;
            }

            let block = if child.kind() == "variable_declarator" {
                self.variable_declarator(child)
            } else {
                self.node(child)
            };

            let spacing = if prev_end_byte.is_none() || matches!(child.kind(), ";" | ",") {
                Spacing::none()
            } else if prev_was_annotated_modifiers && field_anno_wrap != WrapMode::DoNotWrap {
                Spacing::new_line()
            } else if child.start_position().row > prev_end_row.unwrap_or(0) {
                let gap = child.start_position().row - prev_end_row.unwrap_or(0);
                if !is_class_level && prev_was_modifiers && gap > 1 {
                    let capped = gap.min(self.cfg.keep_blank_lines_in_code + 1);
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
            } else if child.kind() == "variable_declarator" {
                Spacing::one_space()
            } else {
                let pb = prev_end_byte.unwrap_or(0);
                if child.start_byte() > pb {
                    let gap = &self.src[pb..child.start_byte()];
                    if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                        Spacing::one_space()
                    } else {
                        Spacing::none()
                    }
                } else {
                    Spacing::none()
                }
            };

            let block = if child.kind() == "variable_declarator"
                && prev_end_row.is_some_and(|pr| child.start_position().row > pr)
                && prev_end_byte.is_some()
            {
                block.with_indent(IndentType::Continuation)
            } else if had_trailing_line_comment_after_modifiers
                && child.kind() != "modifiers"
                && child.kind() != ";"
            {
                had_trailing_line_comment_after_modifiers = false;
                block.with_indent(IndentType::Continuation)
            } else {
                block
            };

            let block = if prev_end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };

            prev_was_modifiers = child.kind() == "modifiers";
            prev_was_annotated_modifiers = child.kind() == "modifiers" && {
                let mut mc = child.walk();
                let kids: Vec<_> = child.children(&mut mc).filter(|c| !c.is_extra()).collect();
                let last = kids.last();
                match last {
                    Some(last) if matches!(last.kind(), "marker_annotation" | "annotation") => {
                        let has_keyword_before = kids.iter().take(kids.len() - 1).any(|c| {
                            !matches!(c.kind(), "marker_annotation" | "annotation")
                                && !c.is_extra()
                                && c.child_count() == 0
                        });
                        if has_keyword_before {
                            false
                        } else if last.kind() == "marker_annotation" {
                            if is_class_level {
                                !self.is_type_use_annotation(*last)
                            } else {
                                !self.is_type_use_annotation_for_align(*last)
                            }
                        } else {
                            true
                        }
                    }
                    _ => false,
                }
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

    pub(crate) fn variable_declarator(&mut self, node: Node) -> Block {
        let in_for_initializer = node.parent().is_some_and(|declaration| {
            declaration.kind() == "local_variable_declaration"
                && declaration
                    .parent()
                    .is_some_and(|parent| parent.kind() == "for_statement")
        });
        let for_initializer_head_overflows = node
            .parent()
            .and_then(|declaration| declaration.parent())
            .filter(|parent| parent.kind() == "for_statement")
            .is_some_and(|for_statement| {
                let head_end = node
                    .child_by_field_name("value")
                    .map_or(node.end_byte(), |value| value.start_byte());
                let prefix = &self.src[for_statement.start_byte()..head_end];
                let mut width = 0usize;
                let mut in_whitespace = false;
                for byte in prefix {
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
                !matches!(
                    self.cfg.for_statement_wrap,
                    crate::config::WrapMode::DoNotWrap
                ) && self.cfg.right_margin > 0
                    && self.compute_output_col(for_statement) + width > self.cfg.right_margin
            });
        let suppress_for_initializer_cont = in_for_initializer && !for_initializer_head_overflows;
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_end_row: Option<usize> = None;
        let mut saw_equals = false;
        let mut prev_ends_with_colon = false;

        for child in node.children(&mut cursor) {
            if child.is_extra() {
                let spacing = if prev_ends_with_colon {
                    Spacing::none()
                } else {
                    Spacing::one_space()
                };
                children.push(self.comment_leaf(child).with_spacing(spacing));
                prev_end_byte = Some(child.end_byte());
                prev_end_row = Some(child.end_position().row);
                prev_ends_with_colon =
                    child.kind() == "ERROR" && self.text(child).trim_end().ends_with(':');
                continue;
            }

            let is_eq = child.kind() == "=";
            if is_eq {
                saw_equals = true;
            }

            let prev_reset_ternary = self.reset_ternary_source_breaks;
            let init_wrap = if saw_equals && !is_eq && prev_end_byte.is_some() {
                let w = Wrap {
                    id: self.ids.wrap_id(),
                    wrap_type: crate::formatter::build::util::wrap_type_from_mode(
                        self.cfg.assignment_wrap,
                    ),
                    wrap_first_element: true,
                    parent: None,
                    ignore_parent_wraps: false,
                    cand_ignore_parents: false,
                };
                self.wraps.insert(w.id, w.clone());
                Some(w)
            } else {
                None
            };
            let prev_enclosing_da = self.enclosing_arg_wrap;
            if let Some(ref w) = init_wrap {
                let src_break_now = prev_end_row.is_some_and(|pr| child.start_position().row > pr);
                if !src_break_now {
                    self.enclosing_arg_wrap = Some(w.id);
                }
            }
            let block = self.node(child);
            self.enclosing_arg_wrap = prev_enclosing_da;
            let block = if let Some(w) = init_wrap {
                block.with_wrap(w)
            } else {
                block
            };
            self.reset_ternary_source_breaks = prev_reset_ternary;

            let source_break = prev_end_row.is_some_and(|pr| child.start_position().row > pr);
            let block = if saw_equals
                && source_break
                && let Some(alignment) = self.for_header_alignment
            {
                block.with_alignment(alignment)
            } else {
                block
            };

            let spacing = if prev_end_byte.is_none() {
                Spacing::none()
            } else if prev_ends_with_colon {
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: 0,
                    keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                    keep_line_breaks: true,
                }
            } else if source_break {
                Spacing::new_line()
            } else {
                let kind = child.kind();
                if kind == "dimensions" {
                    Spacing::none()
                } else {
                    Spacing::one_space()
                }
            };

            let indent = if saw_equals && source_break && !suppress_for_initializer_cont {
                IndentType::Continuation
            } else {
                IndentType::None
            };

            let block = if prev_end_byte.is_some() {
                let block = block.with_spacing(spacing);
                if indent != IndentType::None {
                    block.with_indent(indent)
                } else if saw_equals && !is_eq && !suppress_for_initializer_cont {
                    block.with_break_bonus()
                } else {
                    block
                }
            } else {
                block
            };

            prev_end_byte = Some(child.end_byte());
            prev_end_row = Some(child.end_position().row);
            prev_ends_with_colon =
                matches!(child.kind(), "ERROR" | ":") && self.text(child).trim_end().ends_with(':');
            children.push(block);
        }

        if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        }
    }
}
