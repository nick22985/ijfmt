use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::build::util::wrap_type_from_mode;
use crate::formatter::ir::*;

mod branches;
mod source;

use source::TernarySource;

struct TernaryLayout {
    wrap_id: WrapId,
    wrap_type: WrapType,
    prev_enclosing_arg_wrap: Option<WrapId>,
    prev_in_binary_rhs: bool,
    alignment: Option<TernaryAlignment>,
    align_indent: IndentType,
    effective_signs_on_next: bool,
    signs_on_next: bool,
    nesting: TernaryNesting,
    ternary_arg_suppressed: bool,
    is_paren_direct_child: bool,
}

#[derive(Clone, Copy)]
struct TernaryAlignment {
    operands: AlignmentId,
    signs: AlignmentId,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TernaryNesting {
    Outer,
    Direct,
    Deep,
}

impl TernaryLayout {
    fn wrap(&self) -> Wrap {
        Wrap {
            id: self.wrap_id,
            wrap_type: self.wrap_type,
            wrap_first_element: false,
            parent: self.prev_enclosing_arg_wrap,
            ignore_parent_wraps: false,
            cand_ignore_parents: false,
        }
    }
}

impl<'a> BlockBuilder<'a> {
    pub(crate) fn ternary_expression(&mut self, node: Node) -> Block {
        let cond = node.child_by_field_name("condition");
        let consequence = node.child_by_field_name("consequence");
        let alternative = node.child_by_field_name("alternative");

        let wrap_type = wrap_type_from_mode(self.cfg.ternary_wrap);
        let wrap_id = self.ids.wrap_id();
        let signs_on_next = self.cfg.ternary_operation_signs_on_next_line;
        let is_paren_direct_child = self.paren_direct_child;
        self.paren_direct_child = false;
        let is_nested_ternary = self.ternary_depth > 0;
        let nested_deep = is_nested_ternary && {
            let mut p = node.parent();
            while let Some(pp) = p {
                if pp.kind() == "parenthesized_expression" {
                    p = pp.parent();
                    continue;
                }
                break;
            }
            p.is_none_or(|pp| pp.kind() != "ternary_expression")
        };
        self.ternary_depth += 1;
        let prev_in_binary_rhs = self.in_binary_rhs;
        let prev_binary_rhs_op_row = self.binary_rhs_operator_row;
        let prev_binary_rhs_op_on_new_line = self.binary_rhs_operator_on_new_line;
        let prev_in_broken_ternary_branch = self.in_broken_ternary_branch;
        let prev_enclosing_arg_wrap = self.enclosing_arg_wrap;
        let prev_binary_ctx_wrap = self.binary_ctx_wrap;
        let source_multiline = node.start_position().row != node.end_position().row;

        let same_line_as_enclosing_arg = self.arg_continuation_depth == 1
            && self
                .enclosing_arg_lparen_row
                .is_some_and(|row| row == node.start_position().row);
        let ternary_arg_suppressed = !prev_in_binary_rhs && same_line_as_enclosing_arg;
        let question_on_new_line = cond.is_some_and(|condition| {
            let mut cursor = node.walk();
            node.children(&mut cursor).any(|child| {
                child.kind() == "?" && child.start_position().row > condition.end_position().row
            })
        });
        let ternary_indent = if prev_in_binary_rhs || same_line_as_enclosing_arg {
            if question_on_new_line
                && self.arg_continuation_depth == 1
                && consequence.is_none_or(|branch| branch.kind() != "object_creation_expression")
            {
                IndentType::Continuation
            } else if is_paren_direct_child && !source_multiline {
                IndentType::ContinuationIfWrapped(usize::MAX)
            } else {
                IndentType::None
            }
        } else {
            IndentType::Continuation
        };

        let align_ternary = self.cfg.align_multiline_ternary && source_multiline;
        let alignment = if align_ternary {
            let ops = self.ids.alignment_id();
            let signs = self.ids.alignment_id();
            self.alignment_parents.insert(signs, ops);
            Some(TernaryAlignment {
                operands: ops,
                signs,
            })
        } else {
            None
        };
        let align_indent = ternary_indent;
        let effective_signs_on_next = signs_on_next || align_ternary;

        let source = TernarySource::scan(node, cond, consequence, alternative, source_multiline);
        let layout = TernaryLayout {
            wrap_id,
            wrap_type,
            prev_enclosing_arg_wrap,
            prev_in_binary_rhs,
            alignment,
            align_indent,
            effective_signs_on_next,
            signs_on_next,
            nesting: if nested_deep {
                TernaryNesting::Deep
            } else if is_nested_ternary {
                TernaryNesting::Direct
            } else {
                TernaryNesting::Outer
            },
            ternary_arg_suppressed,
            is_paren_direct_child,
        };
        let mut children = Vec::new();

        self.ternary_condition(&source, &layout, &mut children);

        let mut cons_extra_blocks = Some(self.ternary_consequence_comments(&source, &layout));

        self.ternary_consequence(&source, &layout, &mut children, &mut cons_extra_blocks);

        if let Some(blocks) = cons_extra_blocks.take() {
            children.extend(blocks.into_iter().map(|b| b.with_indent(align_indent)));
        }

        self.ternary_alternative(&source, &layout, &mut children);

        self.ternary_trailing_comments(&source, &mut children);

        self.enclosing_arg_wrap = prev_enclosing_arg_wrap;
        self.binary_ctx_wrap = prev_binary_ctx_wrap;
        self.binary_rhs_operator_row = prev_binary_rhs_op_row;
        self.binary_rhs_operator_on_new_line = prev_binary_rhs_op_on_new_line;
        self.in_broken_ternary_branch = prev_in_broken_ternary_branch;
        self.ternary_depth -= 1;
        let tern_scope = is_nested_ternary;
        if ternary_arg_suppressed {
            let b = Block::node(children)
                .with_bin_anchor()
                .with_tern_self_scope();
            if tern_scope { b.with_tern_scope() } else { b }
        } else {
            let b = Block::node(children).with_tern_self_scope();
            if tern_scope { b.with_tern_scope() } else { b }
        }
    }
    fn ternary_condition(
        &mut self,
        source: &TernarySource<'_>,
        layout: &TernaryLayout,
        children: &mut Vec<Block>,
    ) {
        if let Some(c) = source.cond {
            let w = layout.wrap();
            self.wraps.insert(w.id, w.clone());
            self.positional_wraps.insert(layout.wrap_id);
            let mut cond_block = self.node(c).with_wrap(w);
            if let Some(aid) = layout.alignment.map(|a| a.operands) {
                cond_block = cond_block.with_alignment(aid);
            }
            children.push(cond_block);
        }

        for extra in &source.extras_before_q {
            let on_own_line = source
                .cond
                .is_some_and(|c| extra.start_position().row > c.end_position().row);
            let spacing = if on_own_line {
                Spacing::new_line()
            } else {
                Spacing::one_space()
            };
            let mut cmt = self.comment_leaf(*extra).with_spacing(spacing);
            if !on_own_line {
                cmt = cmt
                    .with_wrap(layout.wrap())
                    .with_indent(layout.align_indent);
                if let Some(aid) = layout.alignment.map(|a| a.operands) {
                    cmt = cmt.with_alignment(aid);
                }
            }
            if on_own_line {
                cmt = cmt.with_indent(layout.align_indent);
                if let Some(aid) = layout.alignment.map(|a| a.operands) {
                    cmt = cmt.with_alignment(aid);
                }
            }
            children.push(cmt);
        }
    }

    fn ternary_consequence_comments(
        &mut self,
        source: &TernarySource<'_>,
        layout: &TernaryLayout,
    ) -> Vec<Block> {
        let mut prev_row = source.consequence.map(|c| c.end_position().row);
        let mut v: Vec<Block> = Vec::new();
        for extra in &source.extras_after_cons {
            let on_own_line = prev_row.is_some_and(|pr| extra.start_position().row > pr);
            let spacing = if on_own_line {
                let row_gap = (extra.start_position().row - prev_row.unwrap_or(0)).max(1);
                let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1);
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: capped as u32,
                    keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                    keep_line_breaks: true,
                }
            } else {
                Spacing::one_space()
            };
            let mut cb = self
                .comment_leaf(*extra)
                .with_spacing(spacing)
                .with_tern_cmt_probe();
            if !on_own_line {
                cb = cb.with_wrap(layout.wrap());
            }
            if on_own_line && let Some(aid) = layout.alignment.map(|a| a.operands) {
                cb = cb.with_alignment(aid);
            }
            v.push(cb);
            prev_row = Some(extra.end_position().row);
        }
        v
    }

    fn ternary_trailing_comments(&mut self, source: &TernarySource<'_>, children: &mut Vec<Block>) {
        let mut prev_row = source.alternative.map(|c| c.end_position().row);
        for extra in &source.extras_after_alt {
            let on_own_line = prev_row.is_some_and(|pr| extra.start_position().row > pr);
            let spacing = if on_own_line {
                let row_gap = extra.start_position().row - prev_row.unwrap_or(0);
                let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1);
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: capped as u32,
                    keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                    keep_line_breaks: true,
                }
            } else {
                Spacing::one_space()
            };
            children.push(self.comment_leaf(*extra).with_spacing(spacing));
            prev_row = Some(extra.end_position().row);
        }
    }
}
