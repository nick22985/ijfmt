use super::*;

impl<'a> BlockBuilder<'a> {
    pub(super) fn ternary_consequence(
        &mut self,
        source: &TernarySource<'_>,
        layout: &TernaryLayout,
        children: &mut Vec<Block>,
        cons_extra_blocks: &mut Option<Vec<Block>>,
    ) {
        if let Some(cons) = source.consequence {
            let w = layout.wrap();
            self.wraps.insert(w.id, w.clone());

            let preserve_source_q = !self.reset_ternary_source_breaks;
            self.in_binary_rhs = true;
            let prev_in_binop_q = self.in_binary_operand;
            self.in_binary_operand = true;
            self.binary_rhs_operator_row = source.q_row;
            self.binary_rhs_operator_on_new_line = source.q_on_new_line;
            self.in_broken_ternary_branch = source.q_on_new_line;
            if layout.effective_signs_on_next && !source.q_at_end_of_line {
                let q_spacing = if preserve_source_q && source.q_on_new_line {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                };
                let cons_after_q_break = preserve_source_q
                    && source.source_multiline
                    && source.q_row.is_some_and(|qr| source.cons_start_row > qr);
                let mut cons_inner = self.node(cons).with_spacing(if cons_after_q_break {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                });
                if cons_after_q_break && layout.alignment.is_none() {
                    cons_inner = cons_inner.with_indent(IndentType::Continuation);
                }
                if layout.nesting == TernaryNesting::Outer
                    && (!cons_after_q_break || layout.alignment.is_some())
                {
                    cons_inner = cons_inner.with_tern_branch_probe();
                }
                if let Some(aid) = layout.alignment.map(|a| a.operands) {
                    cons_inner = cons_inner.with_alignment(aid);
                }
                let mut kids = vec![Block::leaf("?"), cons_inner];
                kids.extend(cons_extra_blocks.take().unwrap_or_default());
                let mut group = Block::node(kids)
                    .with_tern_cmt_scope()
                    .with_tern_self_probe()
                    .with_wrap(w)
                    .with_wrap_no_promote()
                    .with_indent(layout.align_indent)
                    .with_spacing(q_spacing);
                if let Some(aid) = layout.alignment.map(|a| a.signs) {
                    group = group.with_alignment(aid);
                }
                if layout.ternary_arg_suppressed {
                    group = group.with_bin_probe();
                }
                if layout.is_paren_direct_child {
                    group = group.with_tern_colon_probe();
                }
                if layout.nesting != TernaryNesting::Outer {
                    group = group.with_tern_nest_probe();
                    if layout.nesting == TernaryNesting::Deep {
                        group = group.with_tern_nest_deep_probe();
                    }
                }
                children.push(group);
            } else {
                let cons_spacing =
                    if source.q_at_end_of_line || preserve_source_q && source.q_on_new_line {
                        Spacing::new_line()
                    } else {
                        Spacing::one_space()
                    };
                let mut q_leaf = if layout.effective_signs_on_next && source.q_at_end_of_line {
                    Block::leaf("?")
                        .with_spacing(Spacing::one_space())
                        .with_wrap(w.clone())
                        .with_indent(layout.align_indent)
                } else {
                    Block::leaf(" ?").with_spacing(Spacing::none())
                };
                if let Some(aid) = layout.alignment.map(|a| a.signs) {
                    q_leaf = q_leaf.with_alignment(aid);
                }
                children.push(q_leaf);
                let mut cons_block = self
                    .node(cons)
                    .with_wrap(w)
                    .with_indent(layout.align_indent)
                    .with_spacing(cons_spacing);
                if let Some(aid) = layout.alignment.map(|a| a.operands) {
                    cons_block = cons_block.with_alignment(aid);
                }
                if layout.ternary_arg_suppressed {
                    cons_block = cons_block.with_bin_probe();
                }
                if layout.nesting != TernaryNesting::Outer {
                    cons_block = cons_block.with_tern_nest_probe();
                    if layout.nesting == TernaryNesting::Deep {
                        cons_block = cons_block.with_tern_nest_deep_probe();
                    }
                    if layout.alignment.is_none() {
                        cons_block = cons_block.with_tern_nest_col_probe();
                    }
                } else {
                    cons_block = cons_block.with_tern_branch_probe();
                }
                children.push(cons_block);
            }
            self.in_binary_operand = prev_in_binop_q;
            self.in_binary_rhs = layout.prev_in_binary_rhs;
        }
    }

    pub(super) fn ternary_alternative(
        &mut self,
        source: &TernarySource<'_>,
        layout: &TernaryLayout,
        children: &mut Vec<Block>,
    ) {
        if let Some(alt) = source.alternative {
            let prev_in_ternary_alternative = self.in_ternary_alternative;
            self.in_ternary_alternative = true;
            if !layout.signs_on_next {
                self.enclosing_arg_wrap = Some(layout.wrap_id);
                self.binary_ctx_wrap = Some(layout.wrap_id);
            }
            let w = layout.wrap();
            self.wraps.insert(w.id, w.clone());

            let preserve_source_colon = !self.reset_ternary_source_breaks;
            self.in_binary_rhs = true;
            let prev_in_binop_c = self.in_binary_operand;
            self.in_binary_operand = !source.colon_on_new_line;
            self.binary_rhs_operator_row = source.colon_row;
            self.binary_rhs_operator_on_new_line = source.colon_on_new_line;
            self.in_broken_ternary_branch = source.colon_on_new_line;
            let prev_before_colon_row = source
                .extras_after_cons
                .last()
                .map(|e| e.end_position().row)
                .or_else(|| source.consequence.map(|c| c.end_position().row));
            let colon_break_spacing = || -> Spacing {
                let row_gap = source
                    .colon_row
                    .zip(prev_before_colon_row)
                    .map_or(1, |(cr, pr)| cr.saturating_sub(pr));
                let capped = row_gap.min(self.cfg.keep_blank_lines_in_code + 1).max(1);
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: capped as u32,
                    keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                    keep_line_breaks: true,
                }
            };
            let colon_extra_blocks: Vec<Block> = {
                let mut prev_row = source.colon_row;
                let mut v: Vec<Block> = Vec::new();
                for extra in &source.extras_after_colon {
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
                    let mut cb = self.comment_leaf(*extra).with_spacing(spacing);
                    if on_own_line {
                        if let Some(aid) = layout.alignment.map(|a| a.operands) {
                            cb = cb.with_alignment(aid);
                        }
                    } else {
                        cb = cb.with_wrap(w.clone()).with_indent(layout.align_indent);
                    }
                    v.push(cb);
                    prev_row = Some(extra.end_position().row);
                }
                v
            };
            let colon_extras_force_break = source
                .extras_after_colon
                .last()
                .is_some_and(|e| self.text(*e).starts_with("//"));

            if layout.effective_signs_on_next && !source.colon_at_end_of_line {
                let colon_spacing = if preserve_source_colon && source.colon_on_new_line {
                    colon_break_spacing()
                } else {
                    Spacing::one_space()
                };
                let alt_after_colon_break = preserve_source_colon
                    && source.source_multiline
                    && source.colon_row.is_some_and(|cr| source.alt_start_row > cr);
                let mut alt_inner = self.node(alt).with_spacing(
                    if alt_after_colon_break || colon_extras_force_break {
                        Spacing::new_line()
                    } else {
                        Spacing::one_space()
                    },
                );
                if alt_after_colon_break && layout.alignment.is_none() {
                    alt_inner = alt_inner.with_indent(IndentType::Continuation);
                }
                if layout.nesting == TernaryNesting::Outer
                    && (!alt_after_colon_break || layout.alignment.is_some())
                {
                    alt_inner = alt_inner.with_tern_branch_probe();
                }
                if let Some(aid) = layout.alignment.map(|a| a.operands) {
                    alt_inner = alt_inner.with_alignment(aid);
                }
                let mut colon_group_children = vec![Block::leaf(":")];
                colon_group_children.extend(colon_extra_blocks);
                colon_group_children.push(alt_inner);
                let mut group = Block::node(colon_group_children)
                    .with_tern_self_probe()
                    .with_wrap(w)
                    .with_indent(layout.align_indent)
                    .with_spacing(colon_spacing);
                let alternative_ends_in_string = {
                    let mut tail = alt;
                    while tail.kind() == "binary_expression" {
                        let Some(right) = tail.child_by_field_name("right") else {
                            break;
                        };
                        tail = right;
                    }
                    tail.kind() == "string_literal"
                };
                if alt.kind() == "binary_expression"
                    && alternative_ends_in_string
                    && source
                        .consequence
                        .is_some_and(|cons| self.text(cons).trim() == "\"\"")
                    && source.q_row == source.colon_row
                {
                    group = group.with_wrap_no_promote();
                }
                if let Some(aid) = layout.alignment.map(|a| a.signs) {
                    group = group.with_alignment(aid);
                }
                if layout.ternary_arg_suppressed {
                    group = group.with_bin_probe();
                }
                if layout.is_paren_direct_child {
                    group = group.with_tern_colon_probe();
                }
                if layout.nesting != TernaryNesting::Outer {
                    group = group.with_tern_nest_probe();
                    if layout.nesting == TernaryNesting::Deep {
                        group = group.with_tern_nest_deep_probe();
                    }
                }
                children.push(group);
            } else {
                let alt_spacing = if source.colon_at_end_of_line {
                    Spacing::new_line()
                } else if preserve_source_colon && source.colon_on_new_line {
                    colon_break_spacing()
                } else {
                    Spacing::one_space()
                };
                let mut colon_leaf = Block::leaf(" :").with_spacing(Spacing::none());
                if let Some(aid) = layout.alignment.map(|a| a.signs) {
                    colon_leaf = colon_leaf.with_alignment(aid);
                }
                children.push(colon_leaf);
                for cb in colon_extra_blocks {
                    children.push(cb);
                }
                let alt_spacing = if colon_extras_force_break {
                    Spacing::new_line()
                } else {
                    alt_spacing
                };
                let mut alt_block = self
                    .node(alt)
                    .with_wrap(w)
                    .with_indent(layout.align_indent)
                    .with_spacing(alt_spacing);
                if let Some(aid) = layout.alignment.map(|a| a.operands) {
                    alt_block = alt_block.with_alignment(aid);
                }
                if layout.ternary_arg_suppressed {
                    alt_block = alt_block.with_bin_probe();
                }
                if layout.nesting != TernaryNesting::Outer {
                    alt_block = alt_block.with_tern_nest_probe();
                    if layout.nesting == TernaryNesting::Deep {
                        alt_block = alt_block.with_tern_nest_deep_probe();
                    }
                    if layout.alignment.is_none()
                        && source.colon_at_end_of_line
                        && alt.kind() != "method_invocation"
                    {
                        alt_block = alt_block.with_tern_signcol_probe();
                    }
                } else {
                    alt_block = alt_block.with_tern_branch_probe();
                }
                children.push(alt_block);
            }
            self.in_ternary_alternative = prev_in_ternary_alternative;
            self.in_binary_operand = prev_in_binop_c;
            self.in_binary_rhs = layout.prev_in_binary_rhs;
        }
    }
}
