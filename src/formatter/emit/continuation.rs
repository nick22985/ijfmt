use super::super::flatten::{Anchor, ArgCtx};
use super::{
    LineContext, ancestor_line_start, ancestor_line_start_opt, arg_enforced, is_binary_op_text,
};

#[derive(Clone, Copy)]
pub(super) struct ContinuationIndent {
    pub(super) break_bonus_extra: usize,
    pub(super) op_line_flat: Option<usize>,
    pub(super) bin_line_flat: Option<usize>,
    pub(super) bin_child_flat: Option<usize>,
    pub(super) bin_own_line: Option<usize>,
    pub(super) bin_climb_line: Option<usize>,
    pub(super) arr_flat: Option<usize>,
    pub(super) param_flat: Option<usize>,
    pub(super) nested_elem_flat: Option<usize>,
    pub(super) stmt_elem_flat: Option<(usize, bool)>,
    pub(super) tern_cmt_flat: Option<usize>,
    pub(super) tern_self_flat: Option<usize>,
    pub(super) tern_branch_flat: Option<usize>,
    pub(super) paren_cwf_flat: Option<usize>,
    pub(super) tern_sign_flat: Option<usize>,
    pub(super) res_align_override: Option<(usize, usize)>,
    pub(super) empty_list_flat: Option<usize>,
    pub(super) paren_close_flat: Option<usize>,
    pub(super) bin_bonus: usize,
    pub(super) bin_climb_may_lower: bool,
    pub(super) tern_self_may_raise: bool,
}
impl LineContext<'_> {
    pub(super) fn continuation_indent(&self) -> ContinuationIndent {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let debug_binary = self.debug_binary;
        let leaf = &self.leaves[self.i];
        let break_bonus_extra: usize = match leaf.break_bonus {
            Some((vf, gate))
                if facts.starts.get(vf).copied().unwrap_or(false)
                    && gate.is_none_or(|g| facts.starts.get(g).copied().unwrap_or(false)) =>
            {
                config.continuation_indent
            }
            _ => 0,
        };
        let bin_bonus_raw = self.binary_break_bonus();

        let bin_bonus = bin_bonus_raw + break_bonus_extra;
        if debug_binary && (bin_bonus > 0 || leaf.bin_op.is_some()) {
            eprintln!(
                "[BINB] i={} text={:?} raw={} bonus={} bin_op={:?} anchor={}",
                i,
                leaf.text.chars().take(14).collect::<String>(),
                bin_bonus_raw,
                bin_bonus,
                leaf.bin_op,
                match &leaf.anchor {
                    Some(Anchor::Arg(c)) => format!("Arg({})", c.lparen),
                    Some(Anchor::Body(..)) => "Body".into(),
                    None => "None".into(),
                }
            );
        }

        let op_line_flat = self.operand_line_indent();
        let bin_line_flat = self.binary_line_indent();
        let bin_child_flat = self.binary_child_indent();
        let bin_own_line: Option<usize> = match leaf.bin_first {
            Some(bf)
                if bf != i
                    && facts.line_no.get(bf) < facts.line_no.get(i)
                    && facts.line_start_of.get(bf) == Some(&bf)
                    && leaf.arr_elem != Some(bf) =>
            {
                leaf_line_indent
                    .get(bf)
                    .copied()
                    .map(|b| b + config.continuation_indent)
            }
            _ => None,
        };
        let bin_climb_line = self.binary_ancestor_indent();
        let bin_climb_may_lower = is_binary_op_text(&leaf.text)
            && leaf.bin_first.is_some_and(|bf| {
                facts
                    .line_start_of
                    .get(bf)
                    .and_then(|&line_start| leaves.get(line_start))
                    .is_some_and(|opener| {
                        opener.bin_first.is_some() && is_binary_op_text(&opener.text)
                    })
            });
        let arr_chain_bonus = |at: usize, ls: usize| -> usize {
            match &leaf.anchor {
                Some(Anchor::Arg(c)) => match c.chain {
                    Some(id)
                        if facts
                            .chain_first_probe
                            .get(&id)
                            .is_some_and(|&fp| c.lparen >= fp && ls < fp)
                            && facts.chain_dot_breaks.get(&id).is_some_and(|d| {
                                d.iter().any(|&p| p > at) && !d.iter().any(|&p| p <= at)
                            }) =>
                    {
                        config.continuation_indent
                    }
                    _ => 0,
                },
                _ => 0,
            }
        };
        let arr_flat: Option<usize> = match leaf.arr_first {
            Some(bf) if bf != i && facts.line_no.get(bf) < facts.line_no.get(i) => {
                let (ls, ls_moved) = ancestor_line_start_opt(
                    bf,
                    leaves,
                    &facts.line_start_of,
                    leaf_line_indent,
                    config.align_multiline_ternary,
                    false,
                );
                let enforced = match &leaf.anchor {
                    Some(Anchor::Arg(c))
                        if c.lparen < bf
                            && ls_moved
                            && arg_enforced(
                                c,
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
                leaf_line_indent.get(ls).copied().map(|b| {
                    if leaf.arr_close {
                        b + enforced
                    } else {
                        b + enforced + config.continuation_indent + arr_chain_bonus(bf, ls)
                    }
                })
            }
            _ => None,
        };
        let param_flat: Option<usize> = match leaf.param_first {
            Some(bf) if bf != i && facts.line_no.get(bf) < facts.line_no.get(i) => {
                let ls = facts.line_start_of.get(bf).copied().unwrap_or(bf);
                leaf_line_indent
                    .get(ls)
                    .copied()
                    .map(|b| b + config.continuation_indent)
            }
            _ => None,
        };
        let nested_elem_flat: Option<usize> = match &leaf.anchor {
            Some(Anchor::Arg(c))
                if !leaf.anchor_self_exempt
                    && leaf.arg_probe_for == Some(c.lparen)
                    && c.parent.as_ref().is_some_and(|p| !p.smart)
                    && !c
                        .chain
                        .is_some_and(|r| facts.chain_dot_breaks.contains_key(&r))
                    && i > 0 =>
            {
                let anc = facts.line_start_of[i - 1];
                (leaves[anc].span_end >= i
                    && leaves[anc].arg_probe_for.is_some()
                    && facts.line_start_of.get(c.lparen) == Some(&anc))
                .then(|| leaf_line_indent.get(anc).copied())
                .flatten()
                .map(|b| b + config.continuation_indent)
            }
            _ => None,
        };
        let stmt_elem_flat = self.statement_element_indent();
        let tern_cmt_flat: Option<usize> = match leaf.tern_cmt {
            Some(gf) if gf < i && facts.line_no.get(gf) < facts.line_no.get(i) => {
                let ls = facts.line_start_of.get(gf).copied().unwrap_or(gf);
                leaf_line_indent.get(ls).copied().map(|b| {
                    if ls == gf {
                        b
                    } else {
                        b + config.continuation_indent
                    }
                })
            }
            _ => None,
        };
        let tern_self_flat = self.ternary_expression_indent();
        let tern_branch_flat = self.ternary_branch_indent();
        let paren_cwf_flat: Option<usize> = match leaf.paren_cwf {
            Some(lp) if facts.line_no.get(lp) < facts.line_no.get(i) => {
                let ls = ancestor_line_start_opt(
                    lp,
                    leaves,
                    &facts.line_start_of,
                    leaf_line_indent,
                    config.align_multiline_ternary,
                    true,
                )
                .0;
                let enforced = match &leaf.anchor {
                    Some(Anchor::Arg(c))
                        if c.lparen < lp
                            && arg_enforced(
                                c,
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
                leaf_line_indent
                    .get(ls)
                    .map(|b| b + config.continuation_indent + enforced)
            }
            _ => None,
        };
        let tern_sign_flat = self.ternary_sign_indent();
        let res_align_override = self.resource_alignment();
        let empty_list_flat: Option<usize> = if leaf.text == ")"
            && i > 0
            && leaves[i - 1].text == "("
            && facts.line_no.get(i - 1) < facts.line_no.get(i)
        {
            let ls = facts.line_start_of.get(i - 1).copied().unwrap_or(0);
            leaf_line_indent.get(ls).copied()
        } else {
            None
        };
        let tern_self_may_raise = matches!(
            &leaf.anchor,
            Some(Anchor::Arg(context))
                if context.channel
                    && !context.smart
                    && facts.own_break.contains(&context.lparen)
        );
        let paren_close_flat = self.closing_parenthesis_indent();
        ContinuationIndent {
            break_bonus_extra,
            op_line_flat,
            bin_line_flat,
            bin_child_flat,
            bin_own_line,
            bin_climb_line,
            arr_flat,
            param_flat,
            nested_elem_flat,
            stmt_elem_flat,
            tern_cmt_flat,
            tern_self_flat,
            tern_branch_flat,
            paren_cwf_flat,
            tern_sign_flat,
            res_align_override,
            empty_list_flat,
            paren_close_flat,
            bin_bonus,
            bin_climb_may_lower,
            tern_self_may_raise,
        }
    }
}
impl ContinuationIndent {
    pub(super) fn adjust(&self, v: usize) -> usize {
        let v = match self.op_line_flat {
            Some(o) if o < v => o,
            _ => v,
        };
        let v = match self.bin_line_flat {
            Some(o) if o < v => o,
            _ => v,
        };
        let v = match self.bin_child_flat {
            Some(o) if o < v => o,
            _ => v,
        };
        let v = match self.empty_list_flat {
            Some(o) if o < v => o,
            _ => v,
        };
        let v = match self.bin_own_line {
            Some(o) => o,
            None => v,
        };
        let v = match self.bin_climb_line {
            Some(o)
                if self.res_align_override.is_none()
                    && (o > v || (self.bin_climb_may_lower && o < v)) =>
            {
                o
            }
            _ => v,
        };
        let v = match self.paren_cwf_flat {
            Some(o) => o,
            None => v,
        };
        let v = match self.paren_close_flat {
            Some(o) if o < v => o,
            _ => v,
        };
        let v = match self.arr_flat {
            Some(o) => o,
            None => v,
        };
        let v = match self.nested_elem_flat {
            Some(o) if o < v => o,
            _ => v,
        };
        let v = match self.stmt_elem_flat {
            Some((o, true)) => o,
            Some((o, false)) if o < v => o,
            _ => v,
        };
        let v = match self.param_flat {
            Some(o) => o,
            None => v,
        };
        let v = match self.tern_sign_flat {
            Some(o) => o,
            None => v,
        };
        let v = match self.tern_cmt_flat {
            Some(o) => o,
            None => v,
        };
        let v = match self.tern_self_flat {
            Some(o) if o < v || self.tern_self_may_raise => o,
            _ => v,
        };
        match self.tern_branch_flat {
            Some(o) => o,
            None => v,
        }
    }
}

impl LineContext<'_> {
    fn binary_break_bonus(&self) -> usize {
        let i = self.i;
        let config = self.config;
        let facts = self.facts;
        let leaf = &self.leaves[self.i];
        match leaf.bin_op {
            Some((bf, Some(encl), _)) if facts.line_no.get(bf) != facts.line_no.get(encl) => {
                config.continuation_indent
            }
            Some((_, Some(encl), _))
                if facts.own_break_first.get(&encl).is_some_and(|&f| i < f) =>
            {
                config.continuation_indent
            }
            Some((bf, _, Some(lp))) if facts.line_no.get(bf) != facts.line_no.get(lp) => {
                config.continuation_indent
            }
            Some(_)
                if match &leaf.anchor {
                    Some(Anchor::Arg(ctx)) => {
                        let outside = ctx.body.as_ref().map(|(_, _, brace)| *brace);
                        let mut c: Option<&ArgCtx> = Some(ctx);
                        let mut hit = false;
                        while let Some(cc) = c {
                            if outside.is_some_and(|b| cc.lparen < b) {
                                break;
                            }
                            if arg_enforced(
                                cc,
                                i,
                                &facts.elem_firsts,
                                &facts.own_break,
                                &facts.starts,
                            ) {
                                hit = true;
                                break;
                            }
                            c = cc.parent.as_deref();
                        }
                        hit
                    }
                    _ => false,
                } =>
            {
                config.continuation_indent
            }
            _ => 0,
        }
    }
    fn operand_line_indent(&self) -> Option<usize> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf = &self.leaves[self.i];
        if !is_binary_op_text(&leaf.text) {
            match &leaf.anchor {
                Some(Anchor::Arg(c))
                    if !leaf.anchor_self_exempt && leaf.arg_probe_for == Some(c.lparen) =>
                {
                    let ls = facts.line_start_of.get(c.lparen).copied().unwrap_or(0);
                    let chain_bonus = match c.chain {
                        Some(id)
                            if facts
                                .chain_first_probe
                                .get(&id)
                                .is_some_and(|&fp| c.lparen >= fp)
                                && facts.chain_dot_breaks.get(&id).is_some_and(|d| {
                                    d.iter().any(|&p| p > c.lparen)
                                        && !d.iter().any(|&p| p <= c.lparen)
                                }) =>
                        {
                            config.continuation_indent
                        }
                        _ => 0,
                    };
                    (ls != i
                        && is_binary_op_text(&leaves[ls].text)
                        && facts.line_no.get(ls) < facts.line_no.get(i))
                    .then(|| leaf_line_indent.get(ls).copied())
                    .flatten()
                    .map(|b| b + chain_bonus)
                }
                _ => None,
            }
        } else {
            None
        }
    }
    fn binary_line_indent(&self) -> Option<usize> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let chain_roots = self.chain_roots;
        let leaf = &self.leaves[self.i];
        if is_binary_op_text(&leaf.text) {
            match leaf.bin_op {
                Some((bf, _, _)) if bf != i && facts.line_no.get(bf) < facts.line_no.get(i) => {
                    let bf = leaf.bin_first.map_or(bf, |b| b.max(bf));
                    let ls0 = facts.line_start_of.get(bf).copied().unwrap_or(bf);
                    let mut ls = ls0;
                    while ls > 0
                        && leaves[ls].span_end < bf
                        && !leaves[ls].text.trim_start().starts_with('.')
                    {
                        ls = facts.line_start_of.get(ls - 1).copied().unwrap_or(0);
                    }
                    let elem_line = matches!(
                        leaves.get(ls0).and_then(|l| l.anchor.as_ref()),
                        Some(Anchor::Arg(_))
                    );
                    let chain_root = chain_roots.contains(&ls);
                    let enforced = !elem_line
                        && match &leaf.anchor {
                            Some(Anchor::Arg(c)) => {
                                c.smart
                                    && (facts.own_break.contains(&c.lparen)
                                        || facts.rparen_break.contains(&c.lparen))
                            }
                            _ => false,
                        };
                    let mut extra = 0usize;
                    if let Some(Anchor::Arg(ctx)) = &leaf.anchor {
                        let mut c: Option<&ArgCtx> = Some(ctx);
                        let mut innermost = true;
                        while let Some(cc) = c {
                            if !(innermost && enforced)
                                && facts.line_no.get(cc.lparen) >= facts.line_no.get(ls)
                                && arg_enforced(
                                    cc,
                                    i,
                                    &facts.elem_firsts,
                                    &facts.own_break,
                                    &facts.starts,
                                )
                            {
                                extra += config.continuation_indent;
                            }
                            innermost = false;
                            c = cc.parent.as_deref();
                        }
                    }
                    (!chain_root && !facts.broken_chain_line(ls))
                        .then(|| leaf_line_indent.get(ls).copied())
                        .flatten()
                        .map(|b| {
                            b + config.continuation_indent
                                + if enforced {
                                    config.continuation_indent
                                } else {
                                    0
                                }
                                + extra
                        })
                }
                _ => None,
            }
        } else {
            None
        }
    }
    fn binary_child_indent(&self) -> Option<usize> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf = &self.leaves[self.i];
        match leaf.bin_first {
            Some(bf)
                if leaf.bin_op.is_none()
                    && bf != i
                    && facts.line_no.get(bf) < facts.line_no.get(i) =>
            {
                let mut ls = facts.line_start_of.get(bf).copied().unwrap_or(bf);
                while ls > 0 && leaves[ls].span_end < bf {
                    ls = facts.line_start_of.get(ls - 1).copied().unwrap_or(0);
                }
                let enforced = match &leaf.anchor {
                    Some(Anchor::Arg(ctx)) => {
                        arg_enforced(ctx, i, &facts.elem_firsts, &facts.own_break, &facts.starts)
                    }
                    _ => false,
                };
                let mut extra = 0usize;
                if let Some(Anchor::Arg(ctx)) = &leaf.anchor {
                    let mut c: Option<&ArgCtx> = Some(ctx);
                    let mut innermost = true;
                    while let Some(cc) = c {
                        if !(innermost && enforced)
                            && facts.line_no.get(cc.lparen) >= facts.line_no.get(ls)
                            && arg_enforced(
                                cc,
                                i,
                                &facts.elem_firsts,
                                &facts.own_break,
                                &facts.starts,
                            )
                        {
                            extra += config.continuation_indent;
                        }
                        innermost = false;
                        c = cc.parent.as_deref();
                    }
                }
                leaf_line_indent.get(ls).copied().map(|b| {
                    b + config.continuation_indent
                        + if enforced {
                            config.continuation_indent
                        } else {
                            0
                        }
                        + extra
                })
            }
            _ => None,
        }
    }
    fn binary_ancestor_indent(&self) -> Option<usize> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf_align_spaces = &self.history.alignment_spaces;
        let leaf = &self.leaves[self.i];
        match leaf.bin_first {
            Some(bf)
                if bf != i
                    && facts.line_no.get(bf) < facts.line_no.get(i)
                    && facts.line_start_of.get(bf) != Some(&bf) =>
            {
                let ls = ancestor_line_start_opt(
                    bf,
                    leaves,
                    &facts.line_start_of,
                    leaf_line_indent,
                    config.align_multiline_ternary,
                    true,
                )
                .0;
                let opener_is_op = facts
                    .line_start_of
                    .get(bf)
                    .and_then(|&o| leaves.get(o))
                    .is_some_and(|l| l.bin_first.is_some() && is_binary_op_text(&l.text));
                let anc_aligned = leaf.encl_chain.iter().find_map(|&a| {
                    if facts.line_start_of.get(a) == Some(&a) {
                        if matches!(leaves[a].text.as_str(), "?" | ":") {
                            Some(false)
                        } else {
                            Some(leaf_align_spaces.get(a).copied().unwrap_or(0) > 0)
                        }
                    } else {
                        None
                    }
                }) == Some(true);
                if anc_aligned {
                    None
                } else if opener_is_op {
                    let ls = ancestor_line_start_opt(
                        bf,
                        leaves,
                        &facts.line_start_of,
                        leaf_line_indent,
                        config.align_multiline_ternary,
                        true,
                    )
                    .0;
                    leaf_line_indent
                        .get(ls)
                        .copied()
                        .map(|b| b + config.continuation_indent)
                } else {
                    leaf_line_indent
                        .get(ls)
                        .copied()
                        .map(|b| b + config.continuation_indent)
                }
            }
            _ => None,
        }
    }
    fn statement_element_indent(&self) -> Option<(usize, bool)> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf = &self.leaves[self.i];
        match &leaf.anchor {
            Some(Anchor::Arg(c))
                if !leaf.anchor_self_exempt
                    && leaf.arg_probe_for == Some(c.lparen)
                    && !c
                        .chain
                        .is_some_and(|r| facts.chain_dot_breaks.contains_key(&r))
                    && i > 0 =>
            {
                let (anc, _) = ancestor_line_start(
                    i - 1,
                    leaves,
                    &facts.line_start_of,
                    leaf_line_indent,
                    config.align_multiline_ternary,
                );
                let glued_kw = leaves[anc].text == "}"
                    && leaves.get(anc + 1).is_some_and(|l| {
                        matches!(l.text.as_str(), "else" | "catch" | "finally" | "while")
                    });
                let mut ok = (leaves[anc].span_end >= i || glued_kw)
                    && facts.line_start_of.get(c.lparen) == Some(&anc);
                let mut p = c.parent.as_ref();
                while let Some(pp) = p {
                    if pp.lparen < anc {
                        break;
                    }
                    if pp.smart {
                        ok = false;
                        break;
                    }
                    p = pp.parent.as_ref();
                }
                let exact = !is_binary_op_text(&leaves[anc].text)
                    && leaves[anc].text != "?"
                    && !(config.align_multiline_ternary && leaves[anc].text == ":");
                ok.then(|| leaf_line_indent.get(anc).copied())
                    .flatten()
                    .map(|b| (b + config.continuation_indent, exact))
            }
            _ => None,
        }
    }
    fn ternary_expression_indent(&self) -> Option<usize> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf = &self.leaves[self.i];
        match leaf.tern_self {
            Some(bf) if bf != i && facts.line_no.get(bf) < facts.line_no.get(i) => {
                let ls = ancestor_line_start_opt(
                    bf,
                    leaves,
                    &facts.line_start_of,
                    leaf_line_indent,
                    config.align_multiline_ternary,
                    true,
                )
                .0;
                let enforced = match &leaves[bf].anchor {
                    Some(Anchor::Arg(ctx)) => {
                        arg_enforced(ctx, bf, &facts.elem_firsts, &facts.own_break, &facts.starts)
                    }
                    _ => false,
                };
                let dot_line = {
                    let t = leaves[ls].text.trim();
                    t.len() > 1 && t.starts_with('.')
                };
                let chain_extra = match &leaves[bf].anchor {
                    Some(Anchor::Arg(c))
                        if !dot_line
                            && c.chain
                                .is_some_and(|r| facts.chain_dot_breaks.contains_key(&r)) =>
                    {
                        config.continuation_indent
                    }
                    _ => 0,
                };
                leaf_line_indent.get(ls).copied().map(|b| {
                    b + config.continuation_indent
                        + chain_extra
                        + if enforced {
                            config.continuation_indent
                        } else {
                            0
                        }
                })
            }
            _ => None,
        }
    }
    fn ternary_branch_indent(&self) -> Option<usize> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf_start_col = &self.history.start_column;
        let leaf = &self.leaves[self.i];
        match leaf.tern_branch {
            Some(tf)
                if config.align_multiline_ternary
                    && facts.line_no.get(tf) < facts.line_no.get(i) =>
            {
                leaf_start_col.get(tf).copied()
            }
            Some(tf) if facts.line_no.get(tf) < facts.line_no.get(i) => {
                let ls0 = facts.line_start_of.get(tf).copied().unwrap_or(tf);
                let elem_line = leaves
                    .get(ls0)
                    .and_then(|l| l.arg_probe_for)
                    .is_some_and(|lp| {
                        let mut a = match &leaf.anchor {
                            Some(Anchor::Arg(c)) => Some(c),
                            _ => None,
                        };
                        while let Some(c) = a {
                            if c.lparen == lp {
                                return true;
                            }
                            a = c.parent.as_ref();
                        }
                        false
                    });
                let ls = if elem_line {
                    ls0
                } else {
                    ancestor_line_start(
                        tf,
                        leaves,
                        &facts.line_start_of,
                        leaf_line_indent,
                        config.align_multiline_ternary,
                    )
                    .0
                };
                leaf_line_indent
                    .get(ls)
                    .copied()
                    .map(|b| b + config.continuation_indent)
            }
            _ => None,
        }
    }
    fn ternary_sign_indent(&self) -> Option<usize> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf_align_spaces = &self.history.alignment_spaces;
        let leaf = &self.leaves[self.i];
        match &leaf.anchor {
            Some(Anchor::Arg(c)) if leaf.arg_probe_for == Some(c.lparen) => {
                let l0 = facts.line_no.get(c.lparen);
                let mut outer = c;
                let mut same_line = true;
                let mut p = c.parent.as_ref();
                while let Some(pp) = p {
                    if facts.line_no.get(pp.lparen) != l0 {
                        same_line = false;
                    }
                    outer = pp;
                    p = pp.parent.as_ref();
                }
                let ls = facts.line_start_of.get(outer.lparen).copied().unwrap_or(0);
                let sign = leaves[ls].text.trim();
                if same_line
                    && facts.line_no.get(ls) < facts.line_no.get(i)
                    && (sign == "?"
                        || (sign == ":" && leaf_align_spaces.get(ls).copied().unwrap_or(0) > 0))
                {
                    let li = leaf_line_indent.get(ls).copied().unwrap_or(0);
                    let al = leaf_align_spaces.get(ls).copied().unwrap_or(0).min(li);
                    if al > 0 {
                        Some(li - al + config.continuation_indent)
                    } else {
                        Some(li)
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    fn resource_alignment(&self) -> Option<(usize, usize)> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let alignment_anchors = self.alignments;
        {
            let mut ls = facts.line_start_of.get(i).copied().unwrap_or(0);
            let mut hit = None;
            for _ in 0..64 {
                match ls
                    .checked_sub(1)
                    .and_then(|p| facts.line_start_of.get(p).copied())
                {
                    Some(prev) if prev < ls => ls = prev,
                    _ => break,
                }
                let l = match leaves.get(ls) {
                    Some(l) => l,
                    None => break,
                };
                if l.res_align && l.span_end >= i {
                    hit = l
                        .alignment
                        .and_then(|aid| alignment_anchors.inline(aid))
                        .map(|(col, ip)| {
                            (
                                col + config.continuation_indent,
                                ip + config.continuation_indent,
                            )
                        });
                    break;
                }
            }
            hit
        }
    }
    fn closing_parenthesis_indent(&self) -> Option<usize> {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf = &self.leaves[self.i];
        if leaf.paren_close {
            let mut depth = 0i32;
            let mut lp = None;
            for j in (0..i).rev() {
                match leaves[j].text.as_str() {
                    ")" => depth += 1,
                    "(" => {
                        if depth == 0 {
                            lp = Some(j);
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            lp.filter(|&lp| facts.line_no.get(lp) < facts.line_no.get(i))
                .and_then(|lp| {
                    let ls = ancestor_line_start_opt(
                        lp,
                        leaves,
                        &facts.line_start_of,
                        leaf_line_indent,
                        config.align_multiline_ternary,
                        true,
                    )
                    .0;
                    leaf_line_indent.get(ls).copied()
                })
        } else {
            None
        }
    }
}

impl ContinuationIndent {
    pub(super) fn trace_anchor(
        &self,
        i: usize,
        text: &str,
        raw_total: usize,
        adjusted: usize,
        has_override: bool,
    ) {
        eprintln!(
            "[ANCH] i={} text={:?} raw={} adjusted={} some={} op={:?} bl={:?} bc={:?} el={:?} bo={:?} bcl={:?} pc={:?} pcf={:?} arr={:?} ne={:?} se={:?} pf={:?} tsf={:?} tcf={:?} tself={:?} tb={:?}",
            i,
            text,
            raw_total,
            adjusted,
            has_override,
            self.op_line_flat,
            self.bin_line_flat,
            self.bin_child_flat,
            self.empty_list_flat,
            self.bin_own_line,
            self.bin_climb_line,
            self.paren_close_flat,
            self.paren_cwf_flat,
            self.arr_flat,
            self.nested_elem_flat,
            self.stmt_elem_flat,
            self.param_flat,
            self.tern_sign_flat,
            self.tern_cmt_flat,
            self.tern_self_flat,
            self.tern_branch_flat
        );
    }
}
