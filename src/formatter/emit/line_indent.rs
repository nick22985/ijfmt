use super::state::{AlignmentPlacement, DebugFlags};
use super::*;

pub(super) struct IndentEnvironment<'a> {
    pub(super) src: &'a [u8],
    pub(super) alignment_parents: &'a HashMap<AlignmentId, AlignmentId>,
    pub(super) body_anchor_base: &'a mut HashMap<usize, usize>,
    pub(super) debug: &'a DebugFlags,
}

struct AlignmentInheritance {
    ancestor: Option<(usize, usize)>,
    scope_spaces: usize,
}

impl LineContext<'_> {
    pub(super) fn emit_line_indent(
        &self,
        buffer: &mut EmissionBuffer,
        indent: &mut String,
        environment: IndentEnvironment<'_>,
    ) {
        let chain = self.chain_indent(indent);
        let ternary = self.ternary_indent();
        let continuation = self.continuation_indent();
        buffer.alignment_spaces = 0;
        let inheritance = self.inherited_alignment(environment.debug);
        if self.emit_preserved_comment_indent(buffer, indent, environment.src) {
            return;
        }
        let leaf = &self.leaves[self.i];
        if let Some(anchor) = self.alignments.resolve(leaf, environment.alignment_parents) {
            self.emit_registered_alignment(buffer, anchor, environment.debug);
            return;
        }
        if self.emit_expression_alignment(buffer, indent, &ternary, &continuation) {
            return;
        }
        let dot_supplied = self.emit_continuation_indent(
            buffer,
            indent,
            chain,
            &continuation,
            environment.body_anchor_base,
            environment.debug,
        );
        self.apply_inherited_alignment(
            buffer,
            indent,
            inheritance,
            dot_supplied,
            environment.debug,
        );
    }

    fn inherited_alignment(&self, debug: &DebugFlags) -> AlignmentInheritance {
        let i = self.i;
        let leaf = &self.leaves[i];
        let leaves = self.leaves;
        let facts = self.facts;
        let mut ancestor = None;
        for &bf in leaf.encl_chain.iter() {
            if facts.line_start_of.get(bf) == Some(&bf) {
                if is_binary_op_text(&leaves[bf].text) {
                    continue;
                }
                if !matches!(leaves[bf].text.as_str(), "?" | ":") {
                    let spaces = self.history.alignment_spaces.get(bf).copied().unwrap_or(0);
                    ancestor = (spaces > 0).then_some((bf, spaces));
                }
                break;
            }
        }
        let mut align_scope_extra = 0usize;
        if let Some((aid, bf)) = leaf.align_scope
            && facts.line_no.get(bf) < facts.line_no.get(i)
            && facts.line_start_of.get(bf).copied() != Some(bf)
            && let Some((c, p)) = self.alignments.inline(aid)
        {
            let on_different_lines = self
                .alignments
                .leaf(aid)
                .is_none_or(|af| af == bf || facts.line_no.get(af) < facts.line_no.get(bf));
            if c > p && on_different_lines {
                align_scope_extra = c - p;
            }
        }
        if debug.assignment
            && let Some((aid, bf)) = leaf.align_scope
        {
            eprintln!(
                "[AS] i={} text={:?} aid={:?} bf={} bftext={:?} anchors={:?} anchleaf={:?} extra={}",
                i,
                leaf.text.chars().take(12).collect::<String>(),
                aid,
                bf,
                leaves[bf].text.chars().take(12).collect::<String>(),
                self.alignments.inline(aid),
                self.alignments.leaf(aid),
                align_scope_extra
            );
        }

        AlignmentInheritance {
            ancestor,
            scope_spaces: align_scope_extra,
        }
    }

    fn emit_preserved_comment_indent(
        &self,
        buffer: &mut EmissionBuffer,
        indent: &mut String,
        src: &[u8],
    ) -> bool {
        let i = self.i;
        let leaf = &self.leaves[i];
        let leaves = self.leaves;
        let config = self.config;
        let is_first_col_comment = config.keep_first_column_comment
            && !leaf.member_bound_comment
            && (leaf.text.starts_with("//") || leaf.text.starts_with("/*"))
            && leaf
                .source_offset
                .is_some_and(|off| comment_keeps_first_column(src, off));
        if is_first_col_comment {
            buffer.line_indent = 0;
            indent.clear();
        } else if is_bare_block_comment(&leaf.text)
            && !(leaf.member_bound_comment
                && leaves
                    .get(i + 1)
                    .is_some_and(|n| !n.text.starts_with("//") && !n.text.starts_with("/*")))
            && (looks_like_commented_code(&leaf.text)
                || leaf.statement_comment
                || leaf.source_offset.is_some_and(|off| {
                    comment_body_hangs(
                        &leaf.text,
                        source_column(src, off, config),
                        config.indent_size.max(1),
                    ) && comment_binds_to_nothing(src, off + leaf.text.len())
                }))
        {
            if let Some(off) = leaf.source_offset {
                let ws_bytes = source_leading_whitespace(src, off);
                if let Ok(ws_str) = std::str::from_utf8(ws_bytes) {
                    buffer.text.push_str(ws_str);
                } else {
                    buffer.text.push_str(indent);
                }
            } else {
                buffer.text.push_str(indent);
            }
        } else {
            return false;
        }
        true
    }

    fn emit_registered_alignment(
        &self,
        buffer: &mut EmissionBuffer,
        anchor: super::state::RegisteredAlignment,
        debug: &DebugFlags,
    ) {
        let i = self.i;
        let leaf = &self.leaves[i];
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let anchor_col = anchor.column;
        let anchor_indent_part = anchor.indent_part;
        let tabbed_anchor = matches!(
            anchor.placement,
            AlignmentPlacement::Wrapped { pure_tabs: true }
        );
        let mid_line_anchor = matches!(anchor.placement, AlignmentPlacement::Inline);
        if debug.alignment {
            eprintln!(
                "[ALN] i={} text={:?} align={:?} chain={:?} col={} ip={} mid={}",
                i,
                leaf.text,
                leaf.alignment,
                leaf.align_chain,
                anchor_col,
                anchor_indent_part,
                mid_line_anchor
            );
        }
        let tab_size = config.indent_size.max(1);
        let anchor_indent_part = if mid_line_anchor {
            let ind_of = |f: usize| -> usize {
                let li = self.history.line_indent.get(f).copied().unwrap_or(0);
                li - self
                    .history
                    .alignment_spaces
                    .get(f)
                    .copied()
                    .unwrap_or(0)
                    .min(li)
            };
            let chain = &leaf.encl_chain;
            let mut pi = 0usize;
            for (idx, &fi0) in chain.iter().enumerate() {
                let fi = if leaves[fi0].text == "."
                    && facts.starts.get(fi0 + 1).copied().unwrap_or(false)
                {
                    fi0 + 1
                } else {
                    fi0
                };
                if is_binary_op_text(&leaves[fi].text) {
                    continue;
                }
                if facts.starts.get(fi).copied().unwrap_or(false)
                    && facts.line_no.get(fi) < facts.line_no.get(i)
                {
                    pi = ind_of(fi);
                    break;
                }
                let Some(&fnext) = chain.get(idx + 1) else {
                    break;
                };
                if let Some(l) = (fnext..fi).rev().find(|&l| {
                    facts.starts.get(l).copied().unwrap_or(false)
                        && (l == fnext || leaves[l].encl_chain.first() == Some(&fnext))
                }) {
                    pi = ind_of(l);
                    break;
                }
            }
            if pi > anchor_col { 0 } else { pi }
        } else {
            anchor_indent_part
        };
        let align_str = if tabbed_anchor && config.use_tabs && anchor_col.is_multiple_of(tab_size) {
            "\t".repeat(anchor_col / tab_size)
        } else if config.use_tabs && anchor_indent_part > 0 {
            let ip = if anchor_indent_part > anchor_col {
                0
            } else {
                anchor_indent_part
            };
            "\t".repeat(ip / tab_size) + &" ".repeat(ip % tab_size + (anchor_col - ip))
        } else {
            compute_alignment_string(&leaf.indent_stack, anchor_col, config)
        };
        buffer.line_indent = visual_width(&align_str, config.indent_size);
        buffer.alignment_spaces = if tabbed_anchor {
            0
        } else {
            anchor_col.saturating_sub(anchor_indent_part)
        };
        buffer.text.push_str(&align_str);
    }

    fn emit_expression_alignment(
        &self,
        buffer: &mut EmissionBuffer,
        indent: &str,
        ternary: &ternary::TernaryIndent,
        continuation: &continuation::ContinuationIndent,
    ) -> bool {
        let leaf = &self.leaves[self.i];
        let config = self.config;
        if let Some((total, structural)) = continuation.res_align_override {
            let s = if config.use_tabs {
                let ts = config.indent_size.max(1);
                let st = structural.min(total);
                "\t".repeat(st / ts) + &" ".repeat(st % ts + (total - st))
            } else {
                " ".repeat(total)
            };
            buffer.line_indent = total;
            buffer.alignment_spaces = total - structural.min(total);
            buffer.text.push_str(&s);
        } else if let Some(total) = ternary.tern_col_override {
            let s = fill_indent(total, &leaf.indent_stack, config);
            buffer.line_indent = total;
            buffer.text.push_str(&s);
        } else if let Some(total) = ternary
            .tern_nest_override
            .or(ternary.tern_colon_override)
            .filter(|t| *t > visual_width(indent, config.indent_size))
        {
            let s = fill_indent(total, &leaf.indent_stack, config);
            buffer.line_indent = total;
            if let Some(part) = ternary.tern_nest_indent_part.filter(|p| *p < total) {
                buffer.alignment_spaces = total - part;
            }
            buffer.text.push_str(&s);
        } else if let Some((total, indent_part)) = ternary.tern_bin_override {
            let s = if config.use_tabs && indent_part > 0 && indent_part < total {
                let ts = config.indent_size.max(1);
                "\t".repeat(indent_part / ts)
                    + &" ".repeat(indent_part % ts + (total - indent_part))
            } else {
                fill_indent(total, &leaf.indent_stack, config)
            };
            buffer.line_indent = total;
            buffer.alignment_spaces = total.saturating_sub(indent_part);
            buffer.text.push_str(&s);
        } else if let Some(total) = ternary
            .tern_after_sign_override
            .filter(|t| *t > visual_width(indent, config.indent_size))
        {
            let s = fill_indent(total, &leaf.indent_stack, config);
            buffer.line_indent = total;
            buffer.text.push_str(&s);
        } else {
            return false;
        }
        true
    }

    fn emit_continuation_indent(
        &self,
        buffer: &mut EmissionBuffer,
        indent: &str,
        chain: chains::ChainIndent,
        continuation: &continuation::ContinuationIndent,
        body_anchor_base: &mut HashMap<usize, usize>,
        debug: &DebugFlags,
    ) -> bool {
        let i = self.i;
        let leaf = &self.leaves[i];
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let deduplicate_dot_bonus = continuation.break_bonus_extra > 0
            && match leaf.chain_dot {
                Some((root, _, _)) => {
                    leaves.get(root).and_then(|l| l.break_bonus) == leaf.break_bonus
                }
                None => false,
            };
        let mut dot_supplied = false;
        if let Some(ref anchor) = leaf.anchor {
            let anchors::AnchorIndent {
                columns: total,
                deduplicated_break_bonus,
            } = self.anchor_indent(
                anchor,
                anchors::AnchorIndentRequest {
                    indent,
                    break_bonus_extra: continuation.break_bonus_extra,
                    bin_bonus: continuation.bin_bonus,
                    body_anchor_base,
                },
            );
            let lam_bonus = match (leaf.lambda_body_cont, anchor) {
                (Some(bf), Anchor::Arg(ctx))
                    if ctx.lparen < bf
                        && arg_enforced(
                            ctx,
                            i,
                            &facts.elem_firsts,
                            &facts.own_break,
                            &facts.starts,
                        ) =>
                {
                    config.continuation_indent
                }
                _ => 0,
            };
            let dot_wins = match chain.columns {
                Some(o) => {
                    chain.may_lower
                        || o > total.unwrap_or_else(|| visual_width(indent, config.indent_size))
                }
                None => false,
            };
            if dot_wins {
                dot_supplied = true;
            }
            let total = if dot_wins { chain.columns } else { total };
            let bonus = if dot_wins && deduplicate_dot_bonus {
                continuation.bin_bonus - continuation.break_bonus_extra
            } else {
                continuation.bin_bonus
            } + if dot_wins { 0 } else { lam_bonus };
            let bonus = if deduplicated_break_bonus && !dot_wins {
                bonus.saturating_sub(continuation.break_bonus_extra)
            } else {
                bonus
            };
            let total = if bonus > 0 {
                Some(total.unwrap_or_else(|| visual_width(indent, config.indent_size)) + bonus)
            } else {
                total
            };
            let raw_total = total.unwrap_or_else(|| visual_width(indent, config.indent_size));
            let adjusted = continuation.adjust(raw_total);
            if debug.alignment {
                continuation.trace_anchor(i, &leaf.text, raw_total, adjusted, total.is_some());
            }
            if total.is_some() || adjusted != raw_total {
                let anchor_indent = fill_indent(adjusted, &leaf.indent_stack, config);
                buffer.line_indent = adjusted;
                buffer.text.push_str(&anchor_indent);
            } else {
                buffer.line_indent = visual_width(indent, config.indent_size);
                buffer.text.push_str(indent);
            }
        } else if continuation.bin_bonus > 0
            || chain
                .columns
                .is_some_and(|o| chain.may_lower || o > visual_width(indent, config.indent_size))
        {
            let total = match chain.columns {
                Some(o) if chain.may_lower || o > visual_width(indent, config.indent_size) => {
                    dot_supplied = true;
                    o + if deduplicate_dot_bonus {
                        continuation.bin_bonus - continuation.break_bonus_extra
                    } else {
                        continuation.bin_bonus
                    }
                }
                _ => visual_width(indent, config.indent_size) + continuation.bin_bonus,
            };
            let total = continuation.adjust(total);
            let s = fill_indent(total, &leaf.indent_stack, config);
            buffer.line_indent = total;
            buffer.text.push_str(&s);
        } else {
            let v = continuation.adjust(visual_width(indent, config.indent_size));
            if v != visual_width(indent, config.indent_size) {
                let s = fill_indent(v, &leaf.indent_stack, config);
                buffer.line_indent = v;
                buffer.text.push_str(&s);
            } else {
                buffer.line_indent = visual_width(indent, config.indent_size);
                buffer.text.push_str(indent);
            }
        }

        dot_supplied
    }

    fn apply_inherited_alignment(
        &self,
        buffer: &mut EmissionBuffer,
        indent: &str,
        inheritance: AlignmentInheritance,
        dot_supplied: bool,
        debug: &DebugFlags,
    ) {
        let i = self.i;
        let leaf = &self.leaves[i];
        let leaves = self.leaves;
        let config = self.config;
        let (anc_line, anc_align_extra) = inheritance.ancestor.unwrap_or((0, 0));
        if inheritance.scope_spaces > 0 {
            buffer.text.push_str(&" ".repeat(inheritance.scope_spaces));
            buffer.line_indent += inheritance.scope_spaces;
            buffer.alignment_spaces = inheritance.scope_spaces;
        } else if anc_align_extra > 0 && dot_supplied {
            let total = buffer.line_indent.saturating_sub(anc_align_extra);
            let ls = buffer.text.rfind('\n').map(|p| p + 1).unwrap_or(0);
            buffer.text.truncate(ls);
            buffer
                .text
                .push_str(&fill_indent(total, &leaf.indent_stack, config));
            buffer.text.push_str(&" ".repeat(anc_align_extra));
            buffer.line_indent = total + anc_align_extra;
            buffer.alignment_spaces = anc_align_extra;
        } else if anc_align_extra > 0 {
            let bf = anc_line;
            if debug.alignment {
                eprintln!(
                    "[ANC] i={} text={:?} bf={} bftext={:?} li={} als={} struct={} cur={} myst={}",
                    i,
                    leaf.text,
                    bf,
                    leaves[bf].text,
                    self.history.line_indent[bf],
                    self.history.alignment_spaces[bf],
                    self.history.structural_indent[bf],
                    buffer.line_indent,
                    visual_width(indent, config.indent_size)
                );
            }
            let anc_indent_spaces =
                self.history.line_indent[bf].saturating_sub(self.history.alignment_spaces[bf]);
            let child_indent = visual_width(indent, config.indent_size)
                .saturating_sub(self.history.structural_indent[bf]);
            let total = anc_indent_spaces + child_indent;
            let ls = buffer.text.rfind('\n').map(|p| p + 1).unwrap_or(0);
            buffer.text.truncate(ls);
            buffer
                .text
                .push_str(&fill_indent(total, &leaf.indent_stack, config));
            buffer.text.push_str(&" ".repeat(anc_align_extra));
            buffer.line_indent = total + anc_align_extra;
            buffer.alignment_spaces = anc_align_extra;
        }
    }
}
