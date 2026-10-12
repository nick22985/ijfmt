use tree_sitter::Node;

use crate::config::{BraceForce, BraceStyle};
use crate::formatter::build::BlockBuilder;
use crate::formatter::ir::*;

mod spacing;

#[derive(Default)]
struct PreviousControlChild {
    end_byte: Option<usize>,
    end_row: Option<usize>,
    kind: Option<String>,
    bound_kind: Option<String>,
}

struct ControlStatement<'nodes, 'tree> {
    node: Node<'tree>,
    children: &'nodes [Node<'tree>],
    body_indices: &'nodes std::collections::HashSet<usize>,
    force_braces: BraceForce,
    block_containing: bool,
}

struct ControlBodyPlacement {
    is_nonblock_body: bool,
    is_else_if: bool,
    needs_braces: bool,
    keep_one_line: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlCommentPosition {
    BeforeFirst,
    BeforeBlock,
    AfterElse,
}

impl ControlCommentPosition {
    fn after(self, child: Node, needs_braces: bool) -> Self {
        match self {
            Self::BeforeFirst => {
                if child.kind() == "else" {
                    Self::AfterElse
                } else if child.is_extra() || child.kind() == "catch_clause" {
                    Self::BeforeFirst
                } else {
                    Self::BeforeBlock
                }
            }
            Self::BeforeBlock => {
                if child.kind() == "else" {
                    Self::AfterElse
                } else if child.kind() == "block" || child.kind() == "switch_block" || needs_braces
                {
                    Self::BeforeFirst
                } else {
                    Self::BeforeBlock
                }
            }
            Self::AfterElse => Self::BeforeBlock,
        }
    }
}

impl<'a> BlockBuilder<'a> {
    pub(crate) fn statement_with_body(&mut self, node: Node) -> Block {
        let force_braces: BraceForce = match node.kind() {
            "if_statement" => self.cfg.if_brace_force,
            "for_statement" | "enhanced_for_statement" => self.cfg.for_brace_force,
            "while_statement" => self.cfg.while_brace_force,
            "do_statement" => self.cfg.dowhile_brace_force,
            _ => BraceForce::DoNotForce,
        };

        let body_field_ids: Vec<std::num::NonZeroU16> = {
            let lang = node.language();
            let mut ids = Vec::new();
            if let Some(id) = lang.field_id_for_name("body") {
                ids.push(id);
            }
            if let Some(id) = lang.field_id_for_name("consequence") {
                ids.push(id);
            }
            if let Some(id) = lang.field_id_for_name("alternative") {
                ids.push(id);
            }
            ids
        };

        let mut body_indices = std::collections::HashSet::new();
        for &fid in &body_field_ids {
            let mut field_cursor = node.walk();
            for child in node.children_by_field_id(fid, &mut field_cursor) {
                body_indices.insert(child.id());
            }
        }

        let mut children = Vec::new();
        let mut previous = PreviousControlChild::default();

        let for_wrap = if node.kind() == "for_statement"
            && !matches!(
                self.cfg.for_statement_wrap,
                crate::config::WrapMode::DoNotWrap
            ) {
            let w = self.make_wrap(WrapType::Normal);
            self.positional_wraps.insert(w.id);
            Some(w)
        } else {
            None
        };
        let mut for_header_after_semi = false;
        let mut for_header_after_comma = false;
        let for_align = if node.kind() == "for_statement" && self.cfg.align_multiline_for {
            Some(self.ids.alignment_id())
        } else {
            None
        };

        let all_children: Vec<Node> = {
            let mut c = node.walk();
            node.children(&mut c).collect()
        };

        let block_containing = matches!(
            node.kind(),
            "if_statement"
                | "for_statement"
                | "enhanced_for_statement"
                | "while_statement"
                | "do_statement"
                | "try_statement"
                | "catch_clause"
                | "synchronized_statement"
        );
        let mut comment_position = ControlCommentPosition::BeforeFirst;

        let context = ControlStatement {
            node,
            children: &all_children,
            body_indices: &body_indices,
            force_braces,
            block_containing,
        };

        let mut i = 0;
        while i < all_children.len() {
            let child = all_children[i];

            let is_nonblock_body = child.kind() != "block"
                && child.kind() != "switch_block"
                && !child.is_extra()
                && body_indices.contains(&child.id());

            let is_else_if = child.kind() == "if_statement"
                && node.kind() == "if_statement"
                && node
                    .child_by_field_name("alternative")
                    .is_some_and(|alt| alt.id() == child.id());
            let is_braceable_body = is_nonblock_body && !is_else_if && child.kind() != ";";

            if is_else_if && self.force_brace_trailing_owner == Some(node.id()) {
                self.force_brace_trailing_owner = Some(child.id());
            }

            let dependent_lf = is_nonblock_body
                && matches!(
                    node.kind(),
                    "for_statement" | "enhanced_for_statement" | "while_statement"
                )
                && child.end_position().row > child.start_position().row;
            let keep_one_line = self.cfg.keep_control_statement_in_one_line
                && is_nonblock_body
                && !dependent_lf
                && child.start_position().row == node.start_position().row;
            let keep_inline = self.cfg.keep_control_statement_in_one_line
                && is_nonblock_body
                && !dependent_lf
                && previous
                    .end_row
                    .is_some_and(|r| child.start_position().row == r);

            let needs_braces =
                force_braces == BraceForce::Always && is_braceable_body && !keep_one_line;

            if needs_braces
                && self.force_brace_trailing_owner == Some(node.id())
                && child.end_position().row == node.end_position().row
                && matches!(
                    child.kind(),
                    "if_statement" | "for_statement" | "enhanced_for_statement" | "while_statement"
                )
                && child
                    .child_by_field_name("alternative")
                    .or_else(|| child.child_by_field_name("consequence"))
                    .or_else(|| child.child_by_field_name("body"))
                    .is_some_and(|b| b.kind() != "block")
            {
                self.force_brace_trailing_owner = Some(child.id());
            }

            let saved_for_header_alignment = self.for_header_alignment;
            if node.kind() == "for_statement"
                && for_header_after_semi
                && child.kind() == "local_variable_declaration"
            {
                self.for_header_alignment = for_align;
            }
            let block = if child.is_extra() {
                self.control_body_comment(&context, i, previous.end_row, comment_position)
            } else if needs_braces {
                self.force_control_body_braces(node, child, &all_children, &mut i)
            } else {
                self.unbraced_control_body(
                    child,
                    is_nonblock_body,
                    keep_inline,
                    is_else_if,
                    previous.end_row,
                )
            };

            let block = if node.kind() == "switch_expression" && child.kind() == "switch_block" {
                if node
                    .parent()
                    .is_some_and(|parent| parent.kind() == "binary_expression")
                {
                    block.with_indent(IndentType::BlockReset)
                } else {
                    block.with_anchor_body()
                }
            } else {
                block
            };
            self.for_header_alignment = saved_for_header_alignment;

            let (spacing, foreach_colon_space) = self.control_child_spacing(
                &context,
                child,
                &previous,
                ControlBodyPlacement {
                    is_nonblock_body,
                    is_else_if,
                    needs_braces,
                    keep_one_line,
                },
            );

            let block = if previous.end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };
            let block = if foreach_colon_space
                && previous
                    .end_row
                    .is_some_and(|r| child.start_position().row > r)
            {
                block
                    .with_source_line_break()
                    .with_indent(IndentType::Continuation)
            } else {
                block
            };

            let is_for_clause = for_header_after_semi
                && !child.is_extra()
                && !body_indices.contains(&child.id())
                && child.kind() != ")"
                && child.kind() != ";";
            let is_for_comma_clause = for_header_after_comma
                && !child.is_extra()
                && !body_indices.contains(&child.id())
                && child.kind() != ")"
                && child.kind() != ";";
            let block = if is_for_clause || is_for_comma_clause {
                block.with_indent(IndentType::Continuation)
            } else {
                block
            };
            let block = match for_wrap {
                Some(ref w) if is_for_clause => block.with_wrap(w.clone()),
                _ => block,
            };
            let block = match for_align {
                Some(aid) if is_for_clause => block.with_alignment(aid),
                _ => block,
            };
            if is_for_clause {
                for_header_after_semi = false;
            }
            if is_for_comma_clause {
                for_header_after_comma = false;
            }
            if node.kind() == "for_statement"
                && child.kind() == ","
                && !body_indices.contains(&child.id())
            {
                for_header_after_comma = true;
            }

            if node.kind() == "for_statement"
                && (child.kind() == ";"
                    || child.kind() == "local_variable_declaration"
                    || child.kind() == "(")
                && !body_indices.contains(&child.id())
            {
                for_header_after_semi = true;
            }

            comment_position = comment_position.after(child, needs_braces);

            let prev_end_row_before = previous.end_row;
            let effective_end_byte = all_children[i].end_byte();
            let effective_end_row = all_children[i].end_position().row;
            previous.end_byte = Some(effective_end_byte);
            previous.end_row = Some(effective_end_row);
            previous.kind = if needs_braces {
                Some("block".to_string())
            } else {
                Some(child.kind().to_string())
            };
            if !(child.kind() == "block_comment"
                && prev_end_row_before.is_some_and(|r| child.start_position().row == r))
            {
                previous.bound_kind = previous.kind.clone();
            }
            children.push(block);
            i += 1;
        }

        let result = if children.len() == 1 {
            children.pop().unwrap()
        } else {
            Block::node(children)
        };
        if node.kind() == "switch_expression" {
            result.with_anchor_start()
        } else {
            result
        }
    }

    fn control_body_comment(
        &mut self,
        context: &ControlStatement<'_, '_>,
        child_idx: usize,
        prev_end_row: Option<usize>,
        comment_position: ControlCommentPosition,
    ) -> Block {
        let node = context.node;
        let all_children = context.children;
        let child = all_children[child_idx];
        let body_indices = context.body_indices;
        let force_braces = context.force_braces;
        let block_containing = context.block_containing;
        let comment = self.comment_leaf(child);
        let comment_on_own_line = prev_end_row.is_none_or(|r| child.start_position().row > r);
        let mut body_follows_indented = false;
        if comment_on_own_line
            && block_containing
            && comment_position != ControlCommentPosition::BeforeFirst
        {
            let mut part_of_block = false;
            for &n in all_children.iter().skip(child_idx + 1) {
                if n.is_extra() {
                    continue;
                }
                let is_else_if_next = n.kind() == "if_statement"
                    && node.kind() == "if_statement"
                    && node
                        .child_by_field_name("alternative")
                        .is_some_and(|alt| alt.id() == n.id());
                let is_body_non_block = body_indices.contains(&n.id())
                    && n.kind() != "block"
                    && n.kind() != "switch_block"
                    && n.kind() != ";"
                    && !is_else_if_next;
                let next_needs_braces = force_braces == BraceForce::Always
                    && is_body_non_block
                    && child.kind() != "block_comment";
                part_of_block =
                    n.kind() == "block" || n.kind() == "switch_block" || next_needs_braces;
                break;
            }
            body_follows_indented = !part_of_block;
        }
        if body_follows_indented {
            comment.with_indent(IndentType::Normal)
        } else {
            comment
        }
    }

    fn force_control_body_braces(
        &mut self,
        node: Node,
        child: Node,
        all_children: &[Node],
        child_idx: &mut usize,
    ) -> Block {
        let body_block = self.node(child);
        let mut synth_children = vec![
            Block::leaf("{"),
            body_block
                .with_indent(IndentType::Normal)
                .with_spacing(Spacing::new_line()),
        ];

        if child.end_position().row == node.end_position().row
            && self.force_brace_trailing_owner == Some(node.id())
        {
            for comment in self.force_brace_trailing_comments.drain(..) {
                synth_children.push(comment);
            }
            self.force_brace_trailing_owner = None;
        }

        let body_end_row = child.end_position().row;
        while *child_idx + 1 < all_children.len() {
            let next = all_children[*child_idx + 1];
            if next.is_extra() && next.start_position().row == body_end_row {
                synth_children.push(self.comment_leaf(next).with_spacing(Spacing::one_space()));
                *child_idx += 1;
            } else {
                break;
            }
        }

        synth_children.push(Block::leaf("}").with_spacing(Spacing::new_line()));
        Block::node(synth_children)
    }

    fn unbraced_control_body(
        &mut self,
        child: Node,
        is_nonblock_body: bool,
        keep_inline: bool,
        is_else_if: bool,
        prev_end_row: Option<usize>,
    ) -> Block {
        let body = self.node(child);
        let overflow_split = is_nonblock_body
            && !keep_inline
            && !is_else_if
            && prev_end_row.is_some_and(|r| child.start_position().row == r);
        if is_nonblock_body
            && !keep_inline
            && !(is_else_if && self.cfg.special_else_if_treatment)
            && (overflow_split || prev_end_row.is_none_or(|r| child.start_position().row > r))
        {
            body.with_indent(IndentType::Normal)
        } else if is_nonblock_body
            && keep_inline
            && !(is_else_if && self.cfg.special_else_if_treatment)
            && matches!(
                child.kind(),
                "expression_statement"
                    | "return_statement"
                    | "throw_statement"
                    | "break_statement"
                    | "continue_statement"
                    | "yield_statement"
                    | "assert_statement"
                    | "local_variable_declaration"
                    | "if_statement"
                    | "for_statement"
                    | "enhanced_for_statement"
                    | "while_statement"
            )
        {
            let w = Wrap {
                id: self.ids.wrap_id(),
                wrap_type: WrapType::Normal,
                wrap_first_element: true,
                parent: None,
                ignore_parent_wraps: false,
                cand_ignore_parents: false,
            };
            self.wraps.insert(w.id, w.clone());
            body.with_wrap(w)
                .with_indent(IndentType::NormalIfWrapped(usize::MAX))
        } else {
            body
        }
    }
}
