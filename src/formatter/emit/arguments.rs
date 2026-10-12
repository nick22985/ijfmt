use super::super::columns::visual_width;
use super::super::flatten::ArgCtx;
use super::super::ir::IndentType;
use super::anchors::{AnchorIndent, AnchorIndentRequest};
use super::indentation::compute_indent_cols;
use super::{LineContext, ancestor_line_start, is_binary_op_text};
use std::rc::Rc;

impl LineContext<'_> {
    pub(super) fn argument_indent(
        &self,
        ctx: &Rc<ArgCtx>,
        request: AnchorIndentRequest<'_>,
    ) -> AnchorIndent {
        let AnchorIndentRequest {
            indent,
            break_bonus_extra,
            bin_bonus,
            body_anchor_base,
        } = request;
        let mut arg_bb_dedup = false;
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf_align_spaces = &self.history.alignment_spaces;
        let chain_inner_breaks = self.chain_inner_breaks;
        let debug_closer = self.debug_closer;
        let leaf = &self.leaves[self.i];
        let static_total = match &ctx.body {
            Some((cands, bsplit, brace)) => {
                let base = if *brace < i && facts.starts.get(*brace).copied().unwrap_or(false) {
                    leaf_line_indent.get(*brace).copied().unwrap_or(0)
                } else {
                    cands
                        .first()
                        .and_then(|&a| {
                            body_anchor_base
                                .get(&a)
                                .copied()
                                .or_else(|| leaf_line_indent.get(a).copied())
                        })
                        .unwrap_or(0)
                };
                base + compute_indent_cols(
                    leaf.indent_stack
                        .get((*bsplit).min(leaf.indent_stack.len())..)
                        .unwrap_or(&[]),
                    config,
                )
            }
            None => visual_width(indent, config.indent_size),
        };
        let mut total = static_total as isize;
        let cont = config.continuation_indent as isize;
        if let Some(ef) = leaf.arr_elem
            && ef < i
            && facts.line_no.get(ef) < facts.line_no.get(i)
            && (!facts.starts.get(ef).copied().unwrap_or(false)
                || (leaf.bin_first == Some(ef)
                    && leaves
                        .get(ef)
                        .and_then(|l| l.arr_first)
                        .is_some_and(|sf| !facts.starts.get(sf + 1).copied().unwrap_or(true))))
        {
            total -= cont;
        }
        if facts.own_break.contains(&ctx.lparen)
            && leaf
                .indent_stack
                .iter()
                .any(|t| matches!(t, IndentType::LambdaParamGlued))
        {
            total += cont;
        }
        total = self.walk_argument_ancestors(ctx, total, static_total, bin_bonus);
        let mut internal_chain_bonus = 0isize;
        let mut bonus_ctx = Some(ctx);
        while let Some(c) = bonus_ctx {
            if c.chain_channel
                && c.channel
                && !c.elem_has_cont
                && let Some(id) = c.chain
                && let (Some(seg), Some(first)) =
                    (c.seg_first, facts.chain_first_probe.get(&id).copied())
                && seg > first
                && chain_inner_breaks
                    .get(&id)
                    .is_some_and(|points| points.iter().any(|&point| point > c.lparen))
            {
                internal_chain_bonus = cont;
                break;
            }
            bonus_ctx = c.parent.as_ref();
        }
        total += internal_chain_bonus;
        if leaf.anchor_self_exempt && leaf.text == ")" {
            let mut ls = facts
                .line_start_of
                .get(ctx.lparen)
                .copied()
                .unwrap_or(ctx.lparen);
            if ctx
                .parent
                .as_ref()
                .is_some_and(|pp| ls < pp.lparen && (ls..pp.lparen).any(|k| leaves[k].text == ";"))
            {
                ls = ancestor_line_start(
                    ctx.lparen,
                    leaves,
                    &facts.line_start_of,
                    leaf_line_indent,
                    config.align_multiline_ternary,
                )
                .0;
            }
            let outer_lparens = {
                let mut v: Vec<usize> = Vec::new();
                let mut p = Some(ctx);
                while let Some(c) = p {
                    v.push(c.lparen);
                    p = c.parent.as_ref();
                }
                v
            };
            while ls > 0
                && leaves[ls].span_end < ctx.lparen
                && (leaves[ls].text == "="
                    || leaves[ls]
                        .arg_probe_for
                        .is_some_and(|lp| !outer_lparens.contains(&lp)))
            {
                ls = facts.line_start_of.get(ls - 1).copied().unwrap_or(0);
            }
            if debug_closer {
                eprintln!(
                    "CLOSER i={} lparen={} lsline0={:?} ls={} lstext={:?} li={:?} total={}",
                    i,
                    ctx.lparen,
                    facts
                        .line_start_of
                        .get(ctx.lparen)
                        .map(|&x| leaves[x].text.clone()),
                    ls,
                    leaves.get(ls).map(|l| l.text.clone()),
                    leaf_line_indent.get(ls),
                    total
                );
            }
            if let Some(&li) = leaf_line_indent.get(ls) {
                arg_bb_dedup = break_bonus_extra > 0
                    && leaves.get(ls).and_then(|l| l.break_bonus) == leaf.break_bonus;
                let expand = ctx.parent.as_ref().is_some_and(|pp| {
                    facts.cascade_rparen.contains(&pp.lparen)
                        && !facts.own_break.contains(&pp.lparen)
                        && !facts.starts.get(ctx.lparen).copied().unwrap_or(false)
                        && facts.line_no.get(pp.lparen) == facts.line_no.get(ctx.lparen)
                });
                let chain_enforce = ctx.chain.is_some_and(|id| {
                    facts.chain_dot_breaks.contains_key(&id)
                        && id != ls
                        && facts.line_start_of.get(id).copied() == Some(ls)
                });
                let li = li
                    + if expand || chain_enforce {
                        config.continuation_indent
                    } else {
                        0
                    };
                let callee_opens_line = (ls..ctx.lparen).all(|k| {
                    let t = leaves[k].text.as_str();
                    t.starts_with('.')
                        || t == "::"
                        || t == "new"
                        || (!matches!(
                            t,
                            "return"
                                | "throw"
                                | "yield"
                                | "assert"
                                | "if"
                                | "while"
                                | "for"
                                | "switch"
                                | "case"
                                | "do"
                                | "else"
                                | "synchronized"
                                | "catch"
                                | "instanceof"
                        ) && t
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
                });
                let kw_opens_line =
                    matches!(leaves[ls].text.as_str(), "return" | "throw" | "yield")
                        && !ctx
                            .chain
                            .is_some_and(|id| facts.chain_dot_breaks.contains_key(&id));
                let closer_opens_line = leaves[ls].text == ")"
                    && !ctx
                        .chain
                        .is_some_and(|id| facts.chain_dot_breaks.contains_key(&id))
                    && {
                        let mut a = ctx.parent.as_ref();
                        let mut clean = true;
                        while let Some(x) = a {
                            if x.lparen >= ls
                                && x.lparen < ctx.lparen
                                && (facts.own_break.contains(&x.lparen)
                                    || facts.cascade_rparen.contains(&x.lparen))
                            {
                                clean = false;
                                break;
                            }
                            a = x.parent.as_ref();
                        }
                        clean
                    };
                if !is_binary_op_text(leaves[ls].text.trim()) {
                    total = if callee_opens_line || kw_opens_line || closer_opens_line {
                        li as isize
                    } else {
                        total.max(li as isize)
                    };
                }
            }
        }
        if leaf.bin_op.is_some() {
            let chunk_start = |f: usize| -> usize {
                if leaves[f].text == "." && facts.starts.get(f + 1).copied().unwrap_or(false) {
                    f + 1
                } else {
                    f
                }
            };
            if let Some(ef) = leaf.encl_chain.iter().find_map(|&f0| {
                let f = chunk_start(f0);
                (facts.starts.get(f).copied().unwrap_or(false)
                    && facts.line_no.get(f) < facts.line_no.get(i)
                    && !is_binary_op_text(&leaves[f].text))
                .then_some(f)
            }) {
                let base = leaf_line_indent.get(ef).copied().unwrap_or(0)
                    - leaf_align_spaces
                        .get(ef)
                        .copied()
                        .unwrap_or(0)
                        .min(leaf_line_indent.get(ef).copied().unwrap_or(0));
                total = total.max(base as isize + cont - bin_bonus as isize);
            }
        }
        if let Some(bf) = leaf.lambda_body_cont
            && ctx.lparen < bf
        {
            let chunk_start = |f: usize| -> usize {
                if leaves[f].text == "." && facts.starts.get(f + 1).copied().unwrap_or(false) {
                    f + 1
                } else {
                    f
                }
            };
            if let Some(ef) = leaf.encl_chain.iter().find_map(|&f| {
                let g = chunk_start(f);
                (facts.starts.get(g).copied().unwrap_or(false)
                    && facts.line_no.get(g) < facts.line_no.get(i)
                    && !is_binary_op_text(&leaves[g].text))
                .then_some(g)
            }) {
                let li = leaf_line_indent.get(ef).copied().unwrap_or(0);
                let base = li - leaf_align_spaces.get(ef).copied().unwrap_or(0).min(li);
                total = total.max(base as isize + cont);
            }
        }
        let columns = if ctx.body.is_none() && total == static_total as isize {
            None
        } else {
            Some(total.max(0) as usize)
        };
        AnchorIndent {
            columns,
            deduplicated_break_bonus: arg_bb_dedup,
        }
    }
}
