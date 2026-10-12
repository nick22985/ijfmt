use super::{Anchor, LeafEntry, WrapProcessor};
use super::{IndentType, is_binary_op_text};

impl WrapProcessor {
    pub(super) fn compute_end_column(&self, i: usize, leaves: &[LeafEntry]) -> usize {
        self.compute_end_column_inner(i, leaves, 0)
    }

    pub(super) fn compute_end_column_inner(
        &self,
        i: usize,
        leaves: &[LeafEntry],
        depth: usize,
    ) -> usize {
        let mut col = 0usize;

        let mut line_start = 0;
        for j in (0..=i).rev() {
            if j == 0 || leaves[j].white_space.contains_line_feeds() {
                line_start = j;
                break;
            }
        }

        if line_start > 0 || (line_start == 0 && leaves[0].white_space.contains_line_feeds()) {
            col = self
                .compute_indent_width(&leaves[line_start].indent_stack)
                .saturating_sub(self.enforce_deduction(line_start, leaves));
            let mut channel_delta = 0usize;
            if matches!(&leaves[line_start].anchor, Some(Anchor::Arg(_)))
                && !matches!(leaves[line_start].text.as_str(), ")" | "]" | "}")
                && !leaves[line_start]
                    .indent_stack
                    .iter()
                    .any(|x| matches!(x, IndentType::Continuation))
            {
                col += self.continuation_indent;
                channel_delta += self.continuation_indent;
            }
            if channel_delta == 0
                && let Some(Anchor::Arg(ctx)) = &leaves[line_start].anchor
            {
                let mut cur = ctx;
                loop {
                    if cur.collapsed && !cur.channel && !cur.elem_has_cont {
                        let own = (0..=line_start).any(|j| {
                            leaves[j].arg_probe_for == Some(cur.lparen)
                                && (j == line_start || leaves[j].white_space.contains_line_feeds())
                        });
                        if own {
                            col += self.continuation_indent;
                            channel_delta += self.continuation_indent;
                        }
                    }
                    match &cur.parent {
                        Some(p) => cur = p,
                        None => break,
                    }
                }
            }
            if let Some((bf, encl, lp)) = leaves[line_start].bin_op {
                let diff_line = |a: usize, b: usize| {
                    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                    leaves[lo + 1..=hi]
                        .iter()
                        .any(|l| l.white_space.contains_line_feeds())
                };
                if encl.is_some_and(|e| diff_line(bf, e)) || lp.is_some_and(|p| diff_line(bf, p)) {
                    col += self.continuation_indent;
                    channel_delta += self.continuation_indent;
                }
            }
            if let Some(Anchor::Arg(ctx)) = &leaves[line_start].anchor {
                let mut cur = ctx;
                while cur.channel && !cur.elem_has_cont {
                    let Some(pp) = &cur.parent else { break };
                    let same_line = pp.lparen < cur.lparen
                        && !leaves[pp.lparen + 1..=cur.lparen]
                            .iter()
                            .any(|l| l.white_space.contains_line_feeds());
                    let parent_broke = leaves[..line_start].iter().any(|l| {
                        l.white_space.contains_line_feeds() && l.arg_probe_for == Some(pp.lparen)
                    });
                    if !(if same_line { parent_broke } else { true }) {
                        break;
                    }
                    col += self.continuation_indent;
                    channel_delta += self.continuation_indent;
                    cur = pp;
                }
            }
            if depth < 3
                && let Some(Anchor::Arg(ctx)) = &leaves[line_start].anchor
                && ctx.bin_suppressed
                && !ctx.channel
                && !ctx.elem_has_cont
            {
                let own_break = (ctx.lparen + 1..=line_start).any(|j| {
                    leaves[j].arg_probe_for == Some(ctx.lparen)
                        && leaves[j].white_space.contains_line_feeds()
                });
                if own_break && let Some(pp) = ctx.parent.as_ref() {
                    let es = (0..=ctx.lparen)
                        .rev()
                        .find(|&j| leaves[j].arg_probe_for == Some(pp.lparen));
                    let anchor = match es {
                        Some(es) if leaves[es].white_space.contains_line_feeds() => es,
                        _ => pp.lparen,
                    };
                    let b = self.anchor_line_indent(anchor, line_start, leaves, depth);
                    col = col.max(b + self.continuation_indent);
                }
            }
            let mut chain_raised: Option<usize> = None;
            let mut chain_forced: Option<usize> = None;
            let mut dot_won = false;
            if depth < 3
                && let Some((chain_root, 1, _)) = leaves[line_start].chain_dot
                && chain_root < line_start
                && self.channel1_guard(chain_root, leaves)
            {
                let mut root_ls = 0;
                for j in (0..=chain_root).rev() {
                    if j == 0 || leaves[j].white_space.contains_line_feeds() {
                        root_ls = j;
                        break;
                    }
                }
                if root_ls != line_start {
                    let base = self
                        .compute_end_column_inner(root_ls, leaves, depth + 1)
                        .saturating_sub({
                            let t = &leaves[root_ls].text;
                            let last = t.rfind('\n').map(|p| &t[p + 1..]).unwrap_or(t);
                            self.text_width(last)
                        });
                    let unary_logical_rhs = chain_root > root_ls + 1
                        && leaves[chain_root.saturating_sub(24)..chain_root]
                            .windows(2)
                            .any(|pair| pair[0].text.trim() == "(" && pair[1].text.trim() == "!")
                        && leaves[chain_root.saturating_sub(24)..chain_root]
                            .iter()
                            .filter(|leaf| leaf.text.trim() == "!")
                            .take(2)
                            .count()
                            == 2
                        && !matches!(
                            leaves[root_ls].text.as_str(),
                            "if" | "while"
                                | "return"
                                | "for"
                                | "switch"
                                | "throw"
                                | "assert"
                                | "yield"
                        )
                        && leaves[chain_root - 1].text.trim() == "!"
                        && leaves[root_ls..chain_root - 1]
                            .iter()
                            .any(|leaf| matches!(leaf.text.trim(), "&&" | "||"))
                        && self.compute_indent_width(&leaves[line_start].indent_stack) == base;
                    let want = base
                        + if unary_logical_rhs {
                            0
                        } else {
                            self.continuation_indent
                        };
                    let lower_ok = {
                        let elem_start = chain_root > 0
                            && matches!(leaves[chain_root - 1].text.trim(), "(" | "," | "->");
                        let same_line_paren = match &leaves[chain_root].anchor {
                            Some(Anchor::Arg(c)) => {
                                c.lparen <= chain_root
                                    && !leaves[c.lparen + 1..=chain_root]
                                        .iter()
                                        .any(|l| l.white_space.contains_line_feeds())
                            }
                            _ => false,
                        };
                        !(elem_start && same_line_paren)
                    };
                    if lower_ok {
                        dot_won = true;
                        col = want;
                        chain_forced = Some(want);
                    } else {
                        if want > col {
                            dot_won = true;
                        }
                        col = col.max(want);
                    }
                    chain_raised = Some(want);
                }
            }
            if depth < 4 {
                match &leaves[line_start].anchor {
                    Some(Anchor::Body(cands, split, encl, _)) => {
                        if let Some(a) = self.body_base_leaf(cands, encl.as_ref(), leaves)
                            && a < line_start
                        {
                            let base = self.anchor_line_indent(a, line_start, leaves, depth);
                            let local = self.compute_indent_width(
                                leaves[line_start]
                                    .indent_stack
                                    .get((*split).min(leaves[line_start].indent_stack.len())..)
                                    .unwrap_or(&[]),
                            );
                            col = base + local;
                        }
                    }
                    Some(Anchor::Arg(ctx)) => {
                        let lparen_ls = {
                            let mut als = 0;
                            for j in (0..=ctx.lparen).rev() {
                                if j == 0 || leaves[j].white_space.contains_line_feeds() {
                                    als = j;
                                    break;
                                }
                            }
                            als
                        };
                        let lparen_line_is_chain_dot = lparen_ls != line_start
                            && leaves[lparen_ls].chain_dot.is_some()
                            && ctx.seg_first == Some(lparen_ls);
                        let lparen_line_is_tern_sign = lparen_ls != line_start
                            && matches!(leaves[lparen_ls].text.as_str(), ":" | "?");
                        if std::env::var("IJFMT_TERNGATE").is_ok() {
                            eprintln!(
                                "[terngate] ls={} text={:?} lparen={} lparen_ls={} lp_text={:?} elem_has_cont={} channel={} tern={} dot={} seg_first={:?} smart={} col={} base={} lp_end={}",
                                line_start,
                                leaves[line_start].text,
                                ctx.lparen,
                                lparen_ls,
                                leaves[lparen_ls].text,
                                ctx.elem_has_cont,
                                ctx.channel,
                                lparen_line_is_tern_sign,
                                lparen_line_is_chain_dot,
                                ctx.seg_first,
                                ctx.smart,
                                col,
                                self.anchor_line_indent(ctx.lparen, line_start, leaves, depth),
                                leaves
                                    .get(ctx.lparen + 1)
                                    .is_some_and(|l| l.white_space.contains_line_feeds())
                            );
                        }
                        let channel_on_dot_line = ctx.channel
                            && !ctx.elem_has_cont
                            && lparen_ls != line_start
                            && leaves[lparen_ls].chain_dot.is_some()
                            && (leaves
                                .get(ctx.lparen + 1)
                                .is_some_and(|l| l.white_space.contains_line_feeds())
                                || ctx.seg_first.is_none_or(|sf| sf == lparen_ls));
                        let static_arm = !ctx.channel
                            && (ctx.elem_has_cont && lparen_line_is_chain_dot
                                || lparen_line_is_tern_sign);
                        if (static_arm || channel_on_dot_line)
                            && ctx.lparen < line_start
                            && !matches!(leaves[line_start].text.as_str(), ")" | "]" | "}")
                        {
                            let b = self.anchor_line_indent(ctx.lparen, line_start, leaves, depth);
                            let want = b + self.continuation_indent;
                            if lparen_line_is_chain_dot
                                && matches!(leaves[line_start].text.trim(), "?" | ":")
                            {
                                col = want;
                            } else {
                                col = col.max(want);
                            }
                        }
                        if let Some((cands, bsplit, _)) = &ctx.body
                            && let Some(&a) = cands.first()
                            && a < line_start
                        {
                            let base = self.anchor_line_indent(a, line_start, leaves, depth);
                            let local = self.compute_indent_width(
                                leaves[line_start]
                                    .indent_stack
                                    .get((*bsplit).min(leaves[line_start].indent_stack.len())..)
                                    .unwrap_or(&[]),
                            );
                            if std::env::var("IJFMT_BODY_DBG").is_ok() {
                                eprintln!(
                                    "[DBG body] ls={} text={:?} col={} -> base={} local={} chdelta={} split={} stack={:?}",
                                    line_start,
                                    leaves[line_start].text,
                                    col,
                                    base,
                                    local,
                                    channel_delta,
                                    bsplit,
                                    leaves[line_start].indent_stack
                                );
                            }
                            col = col.max(base + local + channel_delta);
                        }
                    }
                    None => {}
                }
                if let Some(c) = chain_forced {
                    col = c;
                } else if let Some(c) = chain_raised {
                    col = col.max(c);
                }
            }
            if depth < 2
                && let Some(aid) = leaves[line_start].alignment
                && let Some(a) = (0..line_start).position(|j| leaves[j].alignment == Some(aid))
                && a < line_start
                && !leaves[a].white_space.contains_line_feeds()
            {
                let end = self.compute_end_column_inner(a, leaves, depth + 1);
                let text = &leaves[a].text;
                let last = text.rfind('\n').map(|p| &text[p + 1..]).unwrap_or(text);
                let anchor_col = end.saturating_sub(self.text_width(last));
                col = col.max(anchor_col);
            }

            if depth < 2
                && let Some(bf) = leaves[line_start].tern_bin
            {
                if bf < line_start {
                    let end = self.compute_end_column_inner(bf, leaves, depth + 1);
                    let text = &leaves[bf].text;
                    let last = text.rfind('\n').map(|p| &text[p + 1..]).unwrap_or(text);
                    col = end.saturating_sub(self.text_width(last));
                } else if bf == line_start
                    && leaves[line_start].tern_branch.is_some()
                    && line_start > 0
                    && leaves[line_start - 1].text.trim() == "?"
                {
                    col = self.leaf_start_column(line_start - 1, leaves);
                }
            }

            if let Some((vf, gate)) = leaves[line_start].break_bonus {
                let gate_open =
                    gate.is_none_or(|g| g == 0 || leaves[g].white_space.contains_line_feeds());
                if gate_open && (vf == 0 || leaves[vf].white_space.contains_line_feeds()) {
                    let dedup = dot_won
                        && match leaves[line_start].chain_dot {
                            Some((cr, _, _)) => {
                                leaves.get(cr).and_then(|l| l.break_bonus)
                                    == leaves[line_start].break_bonus
                            }
                            None => false,
                        };
                    if !dedup {
                        col += self.continuation_indent;
                    }
                }
            }

            if depth < 3
                && !is_binary_op_text(&leaves[line_start].text)
                && let Some(Anchor::Arg(c)) = &leaves[line_start].anchor
            {
                let lp = c.lparen;
                if !leaves[line_start].anchor_self_exempt
                    && leaves[line_start].arg_probe_for == Some(lp)
                    && c.chain.is_none()
                    && lp < line_start
                {
                    let ls2 = (0..=lp)
                        .rev()
                        .find(|&j| j == 0 || leaves[j].white_space.contains_line_feeds())
                        .unwrap_or(0);
                    if ls2 != line_start && is_binary_op_text(&leaves[ls2].text) {
                        let b = self.anchor_line_indent(lp, line_start, leaves, depth);
                        col = col.min(b);
                    }
                }
            }
        }

        if depth < 2 {
            if let Some(lp) = leaves[line_start].paren_cwf
                && lp < line_start
            {
                let ls2 = self.paren_ancestor_line_start(lp, leaves);
                let end = self.compute_end_column_inner(ls2, leaves, depth + 1);
                let text = &leaves[ls2].text;
                let last = text.rfind('\n').map(|p| &text[p + 1..]).unwrap_or(text);
                let base = end.saturating_sub(self.text_width(last));
                col = col.max(base + self.continuation_indent);
            }

            if depth < 2
                && leaves[line_start].anchor.is_none()
                && let Some(binary_first) = leaves[line_start].bin_first
                && leaves[line_start].bin_op.is_none()
                && binary_first < line_start
                && (binary_first + 1..=line_start)
                    .any(|index| leaves[index].white_space.contains_line_feeds())
            {
                let ancestor = self.paren_ancestor_line_start(binary_first, leaves);
                col = self.leaf_start_column(ancestor, leaves) + self.continuation_indent;
            }
        }

        if let Some(lparen) = leaves[line_start].paren_cwf
            && lparen > 0
            && leaves[line_start].wraps.len() >= 2
            && leaves[lparen - 1].white_space.contains_line_feeds()
            && matches!(leaves[lparen - 1].text.trim(), "&&" | "||")
        {
            col = self.leaf_start_column(lparen - 1, leaves);
        }

        col = self.refine_indent(col, line_start, leaves);

        for (j, leaf) in leaves.iter().enumerate().take(i + 1).skip(line_start) {
            if j > line_start && !leaf.white_space.contains_line_feeds() {
                col += leaf.white_space.spaces.max(leaf.pre_run_width);
            }
            let text = &leaf.text;
            if let Some(last_nl) = text.rfind('\n') {
                col = self.text_width(&text[last_nl + 1..]);
            } else {
                col += self.text_width(text);
            }
            col += leaf.extra_width;
        }

        if std::env::var("IJFMT_LS_DBG").is_ok() && depth == 0 {
            eprintln!(
                "[DBG ls] i={} ls={} lstext={:?} col={} pcwf={:?} oscan={:?} orc={:?} stack={:?} ded={} anchor={}",
                i,
                line_start,
                leaves[line_start].text,
                col,
                leaves[line_start].paren_cwf,
                self.indent_oracle
                    .as_ref()
                    .map(|oracle| &oracle.scan)
                    .and_then(|o| o.get(line_start).copied())
                    .flatten(),
                self.indent_oracle
                    .as_ref()
                    .map(|oracle| &oracle.full)
                    .and_then(|o| o.get(line_start).copied())
                    .flatten(),
                leaves[line_start].indent_stack,
                self.enforce_deduction(line_start, leaves),
                match &leaves[line_start].anchor {
                    Some(Anchor::Arg(c)) => format!(
                        "Arg(lp={},ehc={},ch={},bin={},coll={})",
                        c.lparen, c.elem_has_cont, c.channel, c.bin_suppressed, c.collapsed
                    ),
                    Some(Anchor::Body(..)) => "Body".into(),
                    None => "None".into(),
                }
            );
        }
        col
    }

    pub(super) fn text_width(&self, text: &str) -> usize {
        super::super::columns::visual_width(text, self.indent_size)
    }

    pub(super) fn compute_indent_width(&self, stack: &[IndentType]) -> usize {
        let mut width = 0usize;
        for indent_type in stack {
            match indent_type {
                IndentType::None => {}
                IndentType::Normal | IndentType::NormalIfWrapped(_) => {
                    width += self.indent_size;
                }
                IndentType::Continuation => {
                    width += self.continuation_indent;
                }
                IndentType::ContinuationIfWrapped(_) => {
                    width += self.continuation_indent;
                }
                IndentType::Spaces(n) => {
                    width += n;
                }
                IndentType::BlockReset
                | IndentType::BlockResetKeepOne
                | IndentType::CancelOneContinuation
                | IndentType::CancelOneNormal
                | IndentType::ColumnZero
                | IndentType::LambdaParamGlued => {}
            }
        }
        width
    }
}
