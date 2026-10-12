use super::super::columns::visual_width;
use super::super::flatten::{Anchor, ArgCtx};
use super::indentation::compute_indent_cols;
use super::{LineContext, ancestor_line_start, arg_enforced};
use std::collections::HashMap;
use std::rc::Rc;

pub(super) struct AnchorIndentRequest<'a> {
    pub(super) indent: &'a str,
    pub(super) break_bonus_extra: usize,
    pub(super) bin_bonus: usize,
    pub(super) body_anchor_base: &'a mut HashMap<usize, usize>,
}

pub(super) struct AnchorIndent {
    pub(super) columns: Option<usize>,
    pub(super) deduplicated_break_bonus: bool,
}
impl LineContext<'_> {
    pub(super) fn anchor_indent(
        &self,
        anchor: &Anchor,
        request: AnchorIndentRequest<'_>,
    ) -> AnchorIndent {
        match anchor {
            Anchor::Body(cands, split, encl_arg, _) => {
                self.body_indent(cands, split, encl_arg, request)
            }
            Anchor::Arg(ctx) => self.argument_indent(ctx, request),
        }
    }
    fn body_indent(
        &self,
        cands: &[usize],
        split: &usize,
        encl_arg: &Option<Rc<ArgCtx>>,
        request: AnchorIndentRequest<'_>,
    ) -> AnchorIndent {
        let AnchorIndentRequest {
            indent,
            break_bonus_extra,
            body_anchor_base,
            ..
        } = request;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf_start_col = &self.history.start_column;
        let leaf = &self.leaves[self.i];
        let encloses = |lp: usize| {
            let mut cur = encl_arg.as_ref();
            while let Some(c) = cur {
                if c.lparen == lp {
                    return true;
                }
                cur = c.parent.as_ref();
            }
            false
        };
        let anchor_chain = {
            let mut cur = encl_arg.as_ref();
            let mut r = None;
            while let Some(c) = cur {
                if c.chain.is_some() {
                    r = c.chain;
                    break;
                }
                cur = c.parent.as_ref();
            }
            r
        };
        let mut base_leaf = cands.first().copied();
        let mut climb_ctx = encl_arg.clone();
        while let Some(a) = base_leaf {
            if facts.starts.get(a).copied().unwrap_or(false) {
                break;
            }
            let ls = facts.line_start_of.get(a).copied().unwrap_or(0);
            if leaves
                .get(ls)
                .is_some_and(|l| l.text == "?" || (config.align_multiline_ternary && l.text == ":"))
            {
                base_leaf = Some(
                    ancestor_line_start(
                        a,
                        leaves,
                        &facts.line_start_of,
                        leaf_line_indent,
                        config.align_multiline_ternary,
                    )
                    .0,
                );
                break;
            }
            match leaves.get(ls).and_then(|l| l.arg_probe_for) {
                Some(lp) if !encloses(lp) && lp < a => {
                    base_leaf = Some(lp);
                    continue;
                }
                _ => {}
            }
            if leaves.get(ls).is_none_or(|l| l.span_end >= a) {
                break;
            }
            if anchor_chain.is_some()
                && leaves.get(ls).and_then(|l| l.chain_probe_for) == anchor_chain
            {
                break;
            }
            match climb_ctx.as_ref().filter(|c| c.lparen < a && !c.smart) {
                Some(c) => {
                    let lp = c.lparen;
                    climb_ctx = c.parent.clone();
                    base_leaf = Some(lp);
                }
                None => {
                    if let Some(c) = climb_ctx.as_ref().filter(|c| c.lparen < a && c.smart) {
                        if let Some(e) = (0..a).rev().find(|&j| {
                            leaves[j].arg_probe_for == Some(c.lparen)
                                && facts.starts.get(j).copied().unwrap_or(false)
                        }) {
                            base_leaf = Some(e);
                            break;
                        }
                        let elem_of = |x: usize| {
                            facts
                                .elem_firsts
                                .get(&c.lparen)
                                .and_then(|v| v.partition_point(|&e| e <= x).checked_sub(1))
                        };
                        let foreign_chain = leaves
                            .get(ls)
                            .and_then(|l| l.chain_probe_for)
                            .is_some_and(|ch| Some(ch) != c.chain);
                        if (ls <= c.lparen && !foreign_chain) || elem_of(ls) == elem_of(a) {
                            break;
                        }
                        let lp = c.lparen;
                        climb_ctx = c.parent.clone();
                        base_leaf = Some(lp);
                        continue;
                    }
                    let anc = ancestor_line_start(
                        a,
                        leaves,
                        &facts.line_start_of,
                        leaf_line_indent,
                        config.align_multiline_ternary,
                    )
                    .0;
                    let chain_line = anchor_chain.and_then(|ch| {
                        (anc..a).rev().find(|&j| {
                            facts.line_start_of.get(j) == Some(&j)
                                && leaves[j].chain_probe_for == Some(ch)
                        })
                    });
                    base_leaf = Some(chain_line.unwrap_or(anc));
                    break;
                }
            }
        }
        let base = base_leaf
            .and_then(|a| leaf_line_indent.get(a).copied())
            .unwrap_or_else(|| visual_width(indent, config.indent_size));
        let local = compute_indent_cols(
            leaf.indent_stack
                .get((*split).min(leaf.indent_stack.len())..)
                .unwrap_or(&[]),
            config,
        );
        let anchor_mid_line = cands
            .first()
            .is_some_and(|&a| !facts.starts.get(a).copied().unwrap_or(true));
        let line_already_elem = cands.first().is_some_and(|&a| {
            let ls = facts.line_start_of.get(a).copied().unwrap_or(0);
            let mut cur = match &leaves[ls].anchor {
                Some(Anchor::Arg(c)) => Some(c),
                _ => None,
            };
            let target = encl_arg.as_ref().map(|e| e.lparen);
            let mut found = false;
            while let Some(c) = cur {
                if Some(c.lparen) == target {
                    found = true;
                    break;
                }
                cur = c.parent.as_ref();
            }
            found
        });
        let bonus = if anchor_mid_line && !line_already_elem {
            match encl_arg {
                Some(ec)
                    if cands
                        .first()
                        .is_some_and(|&a| facts.line_no.get(a) == facts.line_no.get(ec.lparen)) =>
                {
                    let list_breaks = facts.rparen_break.contains(&ec.lparen)
                        || cands.first().is_some_and(|&a| {
                            arg_enforced(ec, a, &facts.elem_firsts, &facts.own_break, &facts.starts)
                        });
                    let chain_stacks = ec.chain_channel
                        && ec.chain.is_some_and(|chain_id| {
                            let empty = Vec::new();
                            let dots = facts.chain_dot_breaks.get(&chain_id).unwrap_or(&empty);
                            dots.iter().any(|&p| p > ec.lparen)
                                && !dots.iter().any(|&p| p <= ec.lparen)
                                && facts
                                    .chain_first_probe
                                    .get(&chain_id)
                                    .zip(cands.first())
                                    .is_none_or(|(&f, &a)| {
                                        facts.line_start_of.get(f) == facts.line_start_of.get(a)
                                    })
                        });
                    if list_breaks || chain_stacks {
                        config.continuation_indent
                    } else {
                        0
                    }
                }
                _ => 0,
            }
        } else {
            0
        };
        let anon_col = if bonus > 0 {
            cands
                .first()
                .copied()
                .filter(|&a| leaves[a].text == "new")
                .and_then(|a| leaf_start_col.get(a).copied())
        } else {
            None
        };
        if let Some(&a) = cands.first() {
            body_anchor_base.insert(a, anon_col.unwrap_or(base));
        }
        let deduplicated_break_bonus = anon_col.is_none()
            && break_bonus_extra > 0
            && match (leaf.break_bonus, base_leaf) {
                (Some((vf, _)), Some(b)) => b >= vf,
                _ => false,
            };
        let columns = match anon_col {
            Some(c) => c + local,
            None => base + local + bonus,
        };
        AnchorIndent {
            columns: Some(columns),
            deduplicated_break_bonus,
        }
    }
}
