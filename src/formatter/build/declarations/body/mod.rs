use std::collections::HashMap;
use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::ir::*;

mod spacing;
mod special;

use spacing::{ClassBodyLayout, ClassBodyState, MemberCategory, MemberSpacing, simple_enum_body};

impl<'a> BlockBuilder<'a> {
    pub(crate) fn class_body(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut state = ClassBodyState::default();
        let blank_after_header = if node
            .parent()
            .is_some_and(|p| p.kind() == "object_creation_expression")
        {
            self.cfg.blank_lines_after_anonymous_class_header
        } else if node.parent().is_some_and(|p| p.kind() == "enum_constant") {
            0
        } else {
            self.cfg.blank_lines_after_class_header
        };
        let is_interface = node.kind() == "interface_body" || node.kind() == "annotation_type_body";

        let member_spacing = MemberSpacing::new(self.cfg, is_interface);

        let all_body_children: Vec<Node> = node.children(&mut cursor).collect();

        let is_anonymous_class = node
            .parent()
            .is_some_and(|p| p.kind() == "object_creation_expression");
        if is_anonymous_class
            && let Some(body) = self.merged_anonymous_initializer(&all_body_children)
        {
            return body;
        }

        let is_simple_enum_body = simple_enum_body(node, &all_body_children);

        let (field_align, field_cont_align) = if self.cfg.align_group_field_declarations {
            let members: Vec<Node> = all_body_children
                .iter()
                .filter(|c| !matches!(c.kind(), "{" | "}" | "," | ";"))
                .copied()
                .collect();
            self.compute_field_group_alignment(&members)
        } else {
            (HashMap::new(), HashMap::new())
        };

        let layout = ClassBodyLayout {
            node,
            children: &all_body_children,
            simple_enum: is_simple_enum_body,
            blank_after_header,
            member_spacing,
        };

        for (child_idx, &child) in all_body_children.iter().enumerate() {
            if child.is_extra() && matches!(self.md_run_reflowed(child), Some((_, i)) if i > 0) {
                state.end_row = Some(child.end_position().row);
                continue;
            }

            let _is_brace = child.kind() == "{" || child.kind() == "}";

            let mut set_formatter_off_after = false;
            if child.is_extra() && child.kind() == "line_comment" {
                let comment_text = self.text(child);
                if comment_text.contains("@formatter:off") {
                    set_formatter_off_after = true;
                }
            }
            let is_formatter_on = self.formatter_off
                && child.is_extra()
                && child.kind() == "line_comment"
                && self.text(child).contains("@formatter:on");

            let block = if child.kind() == "{" {
                Block::leaf("{")
            } else if child.kind() == "}" {
                Block::leaf("}")
            } else if child.kind() == "," {
                Block::leaf(",")
            } else if child.kind() == ";" {
                Block::leaf(";").with_indent(IndentType::Normal)
            } else if self.formatter_off
                && !set_formatter_off_after
                && !matches!(child.kind(), "enum_body_declarations")
            {
                self.raw_source_block(child)
            } else if child.kind() == "enum_body_declarations" {
                self.enum_body_members(child, member_spacing)
            } else if child.is_extra() {
                self.class_body_comment(&all_body_children, child_idx, state.end_row)
            } else if matches!(child.kind(), "field_declaration" | "constant_declaration") {
                if let Some(&(mp, tp, np)) = field_align.get(&child.id()) {
                    self.field_declaration_aligned(child, mp, tp, np, &field_cont_align)
                        .with_indent(IndentType::Normal)
                } else {
                    self.node(child).with_indent(IndentType::Normal)
                }
            } else {
                self.node(child).with_indent(IndentType::Normal)
            };
            let block = if node.kind() == "enum_body"
                && child.kind() == "enum_constant"
                && self.cfg.class_brace_style == crate::config::BraceStyle::NextLine
            {
                block.with_indent(IndentType::NormalIfWrapped(usize::MAX))
            } else {
                block
            };

            let source_inline_comment = child.is_extra() && child_idx > 0 && {
                let previous = all_body_children[child_idx - 1];
                !self.src[previous.end_byte()..child.start_byte()].contains(&b'\n')
            };
            let spacing =
                self.class_member_spacing(&layout, child_idx, &mut state, source_inline_comment);

            let block = if state.end_row.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };

            if child.is_extra() {
                let on_same_line = source_inline_comment && !self.is_doc_comment_node(child);
                state.was_inline_comment = on_same_line;
            } else {
                state.was_inline_comment = false;
            }
            let is_inline_comment_on_structural = child.is_extra()
                && child.start_position().row == child.end_position().row
                && matches!(state.structural_kind.as_deref(), Some("{") | Some(","))
                && state
                    .end_row
                    .is_some_and(|pr| child.start_position().row <= pr);
            if !is_inline_comment_on_structural {
                state.structural_kind = Some(child.kind().to_string());
            }
            if set_formatter_off_after {
                self.formatter_off = true;
            }
            if is_formatter_on {
                self.formatter_off = false;
            }

            state.end_row = Some(child.end_position().row);
            if let Some(cat) = MemberCategory::from_kind(child.kind()) {
                state.member_category = Some(cat);
            } else if child.kind() == ";" && node.kind() != "enum_body" {
                state.member_category = Some(MemberCategory::StraySemicolon);
            }
            state.kind = Some(child.kind().to_string());
            children.push(block);
        }

        let body = Block::node(children);
        if is_anonymous_class {
            body.with_align_barrier()
        } else {
            body
        }
    }

    fn class_body_comment(
        &mut self,
        all_body_children: &[Node],
        child_idx: usize,
        prev_end_row: Option<usize>,
    ) -> Block {
        let child = all_body_children[child_idx];
        let is_inline = prev_end_row.is_some_and(|pr| child.start_position().row == pr)
            && !self.is_doc_comment_node(child);
        let free_floating = all_body_children
            .get(child_idx + 1)
            .is_none_or(|next| next.start_position().row > child.end_position().row + 1);
        let block = self.comment_leaf(child);
        let block = if free_floating {
            block.with_statement_comment()
        } else {
            block
        };
        if is_inline {
            block
        } else {
            let run_reaches_member = {
                let mut j = child_idx;
                loop {
                    match all_body_children.get(j + 1) {
                        Some(next)
                            if next.start_position().row
                                <= all_body_children[j].end_position().row + 1 =>
                        {
                            if next.is_extra() {
                                j += 1;
                            } else {
                                break next.kind() != "}";
                            }
                        }
                        _ => break false,
                    }
                }
            };
            let first_of_run = child_idx == 0 || {
                let prev = all_body_children[child_idx - 1];
                let prev_own_line = child_idx < 2
                    || all_body_children[child_idx - 2].end_position().row
                        < prev.start_position().row;
                !(prev.is_extra()
                    && prev_own_line
                    && prev.end_position().row + 1 == child.start_position().row)
            };
            let binds = !free_floating && run_reaches_member && first_of_run;
            let block = if binds {
                block.with_member_bound_comment()
            } else {
                block
            };
            block.with_indent(IndentType::Normal)
        }
    }
}
