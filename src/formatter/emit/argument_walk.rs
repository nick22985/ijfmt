use super::super::flatten::ArgCtx;
use super::super::ir::IndentType;
use super::indentation::compute_indent_cols;
use super::{LineContext, ancestor_line_start, arg_enforced, is_binary_op_text};
use std::rc::Rc;

impl LineContext<'_> {
    pub(super) fn walk_argument_ancestors(
        &self,
        ctx: &Rc<ArgCtx>,
        total: isize,
        static_total: usize,
        bin_bonus: usize,
    ) -> isize {
        ArgumentWalk::new(self, ctx, total, static_total, bin_bonus).run()
    }
}

struct ArgumentWalk<'a, 'source> {
    line: &'a LineContext<'source>,
    root: &'a Rc<ArgCtx>,
    body_context: Option<&'a ArgCtx>,
    total: isize,
    static_total: usize,
    bin_bonus: usize,
    attributable: bool,
    innermost_ctx: bool,
    cwf_drop: bool,
    abs_line: Option<usize>,
    chain_bonus_from_walk: Option<usize>,
    inner_scored: bool,
    channel_moved: bool,
    chain_moved: bool,
    standin_pending: bool,
    inner_delta: isize,
}

impl<'a, 'source> ArgumentWalk<'a, 'source> {
    fn new(
        line: &'a LineContext<'source>,
        root: &'a Rc<ArgCtx>,
        total: isize,
        static_total: usize,
        bin_bonus: usize,
    ) -> Self {
        let leaf = &line.leaves[line.i];
        let body_context = if leaf.anchor_self_exempt {
            root.parent.as_deref()
        } else {
            Some(root.as_ref())
        };
        let own_break = line.facts.own_break.contains(&root.lparen);
        let mut walk = Self {
            line,
            root,
            body_context,
            total,
            static_total,
            bin_bonus,
            attributable: false,
            innermost_ctx: true,
            cwf_drop: true,
            abs_line: None,
            chain_bonus_from_walk: None,
            inner_scored: leaf.anchor_self_exempt && root.elem_has_cont && own_break,
            channel_moved: leaf.anchor_self_exempt
                && root.channel
                && !root.elem_has_cont
                && own_break,
            chain_moved: leaf.anchor_self_exempt
                && root
                    .chain
                    .is_some_and(|id| line.expanded_chains.contains(&id)),
            standin_pending: false,
            inner_delta: 0,
        };
        walk.account_for_body_continuations();
        walk
    }

    fn first_context(&self) -> Option<&'a Rc<ArgCtx>> {
        if self.line.leaves[self.line.i].anchor_self_exempt {
            self.root.parent.as_ref()
        } else {
            Some(self.root)
        }
    }

    fn same_body(&self, ctx: &ArgCtx) -> bool {
        match (
            ctx.body.as_ref(),
            self.body_context.and_then(|c| c.body.as_ref()),
        ) {
            (None, None) => true,
            (Some(x), Some(y)) => Rc::ptr_eq(&x.0, &y.0) && x.1 == y.1 && x.2 == y.2,
            _ => false,
        }
    }

    fn chain_expanded(&self, ctx: &ArgCtx) -> bool {
        ctx.chain
            .is_some_and(|id| self.line.expanded_chains.contains(&id))
    }

    fn account_for_body_continuations(&mut self) {
        let mut all = 0;
        let mut dead = 0;
        let mut inside = 0;
        let mut in_body = true;
        let mut context = self.first_context();
        while let Some(c) = context {
            if !self.same_body(c) {
                in_body = false;
            }
            if c.elem_has_cont {
                all += 1;
                if in_body {
                    inside += 1;
                }
                if !in_body && !c.smart {
                    dead += 1;
                }
            }
            context = c.parent.as_ref();
        }
        let count = self.line.leaves[self.line.i]
            .indent_stack
            .iter()
            .filter(|e| **e == IndentType::Continuation)
            .count();
        self.attributable = inside == count;
        if dead > 0 && all == count {
            self.total -= dead as isize * self.line.config.continuation_indent as isize;
        }
    }

    fn run(mut self) -> isize {
        let body_brace = self.root.body.as_ref().map(|(_, _, brace)| *brace);
        let mut context = self.first_context();
        while let Some(c) = context {
            if body_brace.is_some_and(|brace| c.lparen < brace) {
                break;
            }
            self.trace_context(c);
            self.score_continuations(c);
            self.adjust_argument_channel(c);
            self.apply_segment_floor(c);
            self.adjust_chain_channel(c);
            if self.anchor_innermost_argument(c) {
                break;
            }
            if self.line.debug_anchor {
                eprintln!("   -> after lparen={} total={}", c.lparen, self.total);
            }
            self.innermost_ctx = false;
            context = c.parent.as_ref();
        }
        self.total
    }

    fn argument_scores(&self, c: &ArgCtx) -> bool {
        let i = self.line.i;
        let leaves = self.line.leaves;
        let facts = self.line.facts;
        match (0..=i)
            .rev()
            .find(|&j| leaves[j].arg_probe_for == Some(c.lparen))
        {
            Some(es) => {
                es == i
                    || facts.starts.get(es).copied().unwrap_or(false)
                    || (c.lparen + 1..i).any(|p| {
                        facts.starts.get(p).copied().unwrap_or(false) && leaves[p].span_end >= i
                    })
            }
            None => true,
        }
    }

    fn trace_context(&self, c: &Rc<ArgCtx>) {
        let i = self.line.i;
        let leaves = self.line.leaves;
        let facts = self.line.facts;
        let leaf = &leaves[i];
        let debug_anchor = self.line.debug_anchor;
        if debug_anchor {
            eprintln!(
                "ARG leaf={:?} static={} total={} binb={} c.lparen={} smart={} channel={} elem_has_cont={} own_break={} chain={:?} cc={} cws={} fp={:?} dots={:?} stack={:?}",
                leaf.text.chars().take(12).collect::<String>(),
                self.static_total,
                self.total,
                self.bin_bonus,
                c.lparen,
                c.smart,
                c.channel,
                c.elem_has_cont,
                facts.own_break.contains(&c.lparen),
                c.chain,
                c.chain_channel,
                c.chain_wrapper_static,
                c.chain
                    .and_then(|x| facts.chain_first_probe.get(&x).copied()),
                c.chain
                    .and_then(|x| facts.chain_dot_breaks.get(&x).cloned()),
                leaf.indent_stack
            );
        }
    }

    fn score_continuations(&mut self, c: &Rc<ArgCtx>) {
        let i = self.line.i;
        let leaves = self.line.leaves;
        let config = self.line.config;
        let facts = self.line.facts;
        let leaf_line_indent = &self.line.history.line_indent;
        let leaf = &leaves[i];
        let cont = config.continuation_indent as isize;
        if self.standin_pending {
            self.standin_pending = false;
            if !(c.lparen + 1..i)
                .any(|p| facts.starts.get(p).copied().unwrap_or(false) && leaves[p].span_end >= i)
            {
                self.channel_moved = false;
                self.inner_scored = true;
            }
        }
        if self.cwf_drop && !self.innermost_ctx && leaf.arg_probe_for.is_some() {
            let es = (0..=i)
                .rev()
                .find(|&j| leaves[j].arg_probe_for == Some(c.lparen));
            match es {
                Some(es) if es == i || facts.starts.get(es).copied().unwrap_or(false) => {
                    self.cwf_drop = false;
                }
                Some(_)
                    if (c.lparen + 1..i).any(|p| {
                        facts.starts.get(p).copied().unwrap_or(false) && leaves[p].span_end >= i
                    }) =>
                {
                    self.cwf_drop = false;
                }
                Some(_) if !c.smart && c.elem_has_cont && facts.own_break.contains(&c.lparen) => {
                    self.total -= cont;
                }
                _ => {}
            }
        }
        if (!self.innermost_ctx || leaf.anchor_self_exempt)
            && self.attributable
            && self.same_body(c)
            && !self.channel_moved
            && !self.chain_moved
            && self.inner_scored
            && !c.channel
            && c.smart
            && c.elem_has_cont
            && !facts.own_break.contains(&c.lparen)
            && !facts.cascade_rparen.contains(&c.lparen)
        {
            self.total -= cont;
        }
        if self.channel_moved
            && !self.innermost_ctx
            && self.attributable
            && self.same_body(c)
            && !self.chain_moved
            && !c.channel
            && c.smart
            && c.elem_has_cont
            && !facts.own_break.contains(&c.lparen)
            && !facts.cascade_rparen.contains(&c.lparen)
        {
            self.standin_pending = true;
        }
        if c.elem_has_cont && facts.own_break.contains(&c.lparen) {
            self.inner_scored = true;
        }
        if self.innermost_ctx
            && c.elem_has_cont
            && facts.own_break.contains(&c.lparen)
            && leaf.arg_probe_for == Some(c.lparen)
        {
            let lp_line = facts
                .line_start_of
                .get(c.lparen)
                .copied()
                .unwrap_or(c.lparen);
            if leaves[lp_line].chain_dot.is_some() {
                let emitted = leaf_line_indent.get(lp_line).copied().unwrap_or(0) as isize;
                let stat = compute_indent_cols(&leaves[lp_line].indent_stack, config) as isize;
                if emitted < stat {
                    self.total -= stat - emitted;
                }
            }
        }
        if c.channel && !c.elem_has_cont && facts.own_break.contains(&c.lparen) {
            self.channel_moved = true;
        }
        if self.chain_expanded(c) {
            self.chain_moved = true;
        }
        if self.innermost_ctx
            && !leaf.anchor_self_exempt
            && leaf.chain_dot.is_some()
            && c.elem_has_cont
            && !c.smart
        {
            let scores = self.argument_scores(c);
            if !scores {
                self.total -= cont;
            }
        }
        if self.innermost_ctx
            && !leaf.anchor_self_exempt
            && leaf.chain_dot.is_some()
            && c.elem_has_cont
            && c.smart
            && !c.channel
            && !c.chain_channel
            && self.chain_expanded(c)
            && !facts.own_break.contains(&c.lparen)
            && !facts.cascade_rparen.contains(&c.lparen)
        {
            self.total -= cont;
        }
        let nested_levels = {
            let mut n = 0usize;
            let mut p = Some(c);
            while let Some(x) = p {
                if !self.same_body(x) {
                    break;
                }
                if x.elem_has_cont {
                    n += 1;
                }
                p = x.parent.as_ref();
            }
            n
        };
        if self.innermost_ctx
            && self.attributable
            && nested_levels >= 2
            && !leaf.anchor_self_exempt
            && leaf.chain_dot.is_none()
            && c.elem_has_cont
            && c.smart
            && !c.channel
            && !c.chain_channel
            && !facts.own_break.contains(&c.lparen)
            && !facts.cascade_rparen.contains(&c.lparen)
        {
            let scores = self.argument_scores(c);
            if !scores {
                self.total -= cont;
            }
        }
    }

    fn adjust_argument_channel(&mut self, c: &Rc<ArgCtx>) {
        let i = self.line.i;
        let leaves = self.line.leaves;
        let config = self.line.config;
        let facts = self.line.facts;
        let leaf_line_indent = &self.line.history.line_indent;
        let leaf = &leaves[i];
        let cont = config.continuation_indent as isize;
        if c.collapsed && !c.channel && !c.elem_has_cont && facts.own_break.contains(&c.lparen) {
            self.total += cont;
        } else if c.bin_suppressed
            && !c.channel
            && !c.elem_has_cont
            && facts.own_break.contains(&c.lparen)
        {
            let base = match c.parent.as_ref() {
                Some(pp) => {
                    let es = (0..=c.lparen)
                        .rev()
                        .find(|&j| leaves[j].arg_probe_for == Some(pp.lparen));
                    match es {
                        Some(es) if facts.starts.get(es).copied().unwrap_or(false) => {
                            leaf_line_indent.get(es).copied()
                        }
                        _ => leaf_line_indent.get(pp.lparen).copied().map(|b| {
                            if arg_enforced(
                                pp,
                                i,
                                &facts.elem_firsts,
                                &facts.own_break,
                                &facts.starts,
                            ) {
                                b + config.continuation_indent
                            } else {
                                b
                            }
                        }),
                    }
                }
                None => {
                    let (ls, _) = ancestor_line_start(
                        c.lparen,
                        leaves,
                        &facts.line_start_of,
                        leaf_line_indent,
                        config.align_multiline_ternary,
                    );
                    let ls = leaves
                        .get(ls)
                        .filter(|l| is_binary_op_text(&l.text))
                        .and_then(|l| l.bin_first)
                        .and_then(|bf| facts.line_start_of.get(bf).copied())
                        .unwrap_or(ls);
                    leaf_line_indent.get(ls).copied()
                }
            };
            if let Some(b) = base {
                let want = (b + config.continuation_indent) as isize - self.bin_bonus as isize
                    + self.inner_delta;
                self.total = self.total.max(want);
            }
        } else if c.channel
            && facts.own_break.contains(&c.lparen)
            && !self
                .abs_line
                .is_some_and(|ls| facts.line_start_of.get(c.lparen).copied().unwrap_or(0) <= ls)
            && !(self.attributable
                && c.parent.as_ref().is_some_and(|pp| {
                    let ls = facts
                        .line_start_of
                        .get(c.lparen)
                        .copied()
                        .unwrap_or(c.lparen);
                    ls > pp.lparen && leaves[ls].span_end < c.lparen
                })
                && {
                    let mut p = c.parent.as_ref();
                    let mut standin = false;
                    while let Some(pp) = p {
                        if pp.elem_has_cont
                            && !facts.own_break.contains(&pp.lparen)
                            && !facts.cascade_rparen.contains(&pp.lparen)
                        {
                            standin = true;
                            break;
                        }
                        if facts.own_break.contains(&pp.lparen)
                            || facts.cascade_rparen.contains(&pp.lparen)
                        {
                            break;
                        }
                        p = pp.parent.as_ref();
                    }
                    standin
                })
        {
            let desired = c.parent.as_ref().is_some_and(|pp| {
                (facts.line_no.get(pp.lparen) == facts.line_no.get(c.lparen)
                    && (facts.own_break.contains(&pp.lparen)
                        || facts.cascade_rparen.contains(&pp.lparen)))
                    || facts.line_no.get(pp.lparen) != facts.line_no.get(c.lparen)
            });
            let cwf_scores = c.smart || self.argument_scores(c);
            if desired && !c.elem_has_cont && cwf_scores {
                let mut extra = 0isize;
                if !leaf.indent_stack.contains(&IndentType::Continuation) {
                    let mut p = c.parent.as_ref();
                    while let Some(pp) = p {
                        if pp.elem_has_cont
                            && arg_enforced(
                                pp,
                                i,
                                &facts.elem_firsts,
                                &facts.own_break,
                                &facts.starts,
                            )
                        {
                            extra += cont;
                        }
                        p = pp.parent.as_ref();
                    }
                }
                self.total += cont + extra;
                self.inner_delta += cont + extra;
            } else if !desired && c.elem_has_cont {
                self.total -= cont;
            } else if !desired && !c.elem_has_cont {
                let lp_line0 = facts
                    .line_start_of
                    .get(c.lparen)
                    .copied()
                    .unwrap_or(c.lparen);
                let sibling_opener = c.parent.as_ref().is_some_and(|pp| {
                    lp_line0 < pp.lparen && (lp_line0..pp.lparen).any(|k| leaves[k].text == ";")
                });
                let lp_line = if sibling_opener {
                    ancestor_line_start(
                        c.lparen,
                        leaves,
                        &facts.line_start_of,
                        leaf_line_indent,
                        config.align_multiline_ternary,
                    )
                    .0
                } else {
                    lp_line0
                };
                let lparen_line_indent = leaf_line_indent.get(lp_line).copied().unwrap_or(0);
                let mut extra = 0isize;
                if leaf.arg_probe_for == Some(c.lparen) {
                    let mut p = c.parent.as_ref();
                    while let Some(pp) = p {
                        if facts.line_no.get(pp.lparen) != facts.line_no.get(c.lparen) {
                            break;
                        }
                        match (0..=i)
                            .rev()
                            .find(|&j| leaves[j].arg_probe_for == Some(pp.lparen))
                        {
                            Some(e) if facts.starts.get(e).copied().unwrap_or(false) => {
                                break;
                            }
                            Some(_) if facts.own_break.contains(&pp.lparen) => {
                                extra += cont;
                            }
                            _ => {}
                        }
                        p = pp.parent.as_ref();
                    }
                }
                if extra > 0 && self.total < lparen_line_indent as isize + extra + cont {
                    self.total = lparen_line_indent as isize + extra + cont;
                    self.abs_line = Some(lp_line);
                } else if self.total <= lparen_line_indent as isize {
                    self.total = lparen_line_indent as isize + cont;
                    self.abs_line = Some(lp_line);
                }
            }
        }
    }

    fn apply_segment_floor(&mut self, c: &Rc<ArgCtx>) {
        let i = self.line.i;
        let leaves = self.line.leaves;
        let config = self.line.config;
        let facts = self.line.facts;
        let leaf_line_indent = &self.line.history.line_indent;
        let leaf = &leaves[i];
        if c.seg_cancel && !(leaf.anchor_self_exempt && leaf.text == ")") {
            if let Some(cr) = c.chain.filter(|_| c.chain_wrapper_static)
                && cr < i
                && facts.starts.get(cr).copied().unwrap_or(false)
                && let Some(&rl) = leaf_line_indent.get(cr)
            {
                let broke = facts
                    .chain_dot_breaks
                    .get(&cr)
                    .is_some_and(|d| !d.is_empty());
                let want = rl
                    + if broke { config.continuation_indent } else { 0 }
                    + if c.elem_has_cont {
                        config.continuation_indent
                    } else {
                        0
                    };
                self.total = self.total.max(want as isize);
            }
            if let Some(sf) = c.seg_first {
                if facts.starts.get(sf).copied().unwrap_or(false) {
                    if let Some(&dl) = leaf_line_indent.get(sf) {
                        self.total = self.total.max((dl + config.continuation_indent) as isize);
                    }
                } else if let Some(dl) = c
                    .chain
                    .and_then(|id| facts.chain_dot_breaks.get(&id))
                    .and_then(|dots| dots.iter().copied().filter(|&p| p < sf).max())
                    .and_then(|d| leaf_line_indent.get(d).copied())
                {
                    self.total = self.total.max((dl + config.continuation_indent) as isize);
                }
            }
        }
    }

    fn adjust_chain_channel(&mut self, c: &Rc<ArgCtx>) {
        let i = self.line.i;
        let leaves = self.line.leaves;
        let config = self.line.config;
        let facts = self.line.facts;
        let leaf = &leaves[i];
        let cont = config.continuation_indent as isize;
        let ws = &leaf.white_space;
        if c.chain_channel
            && let Some(chain_id) = c.chain
            && (facts.own_break.contains(&c.lparen) || ws.line_feeds > 0)
        {
            let empty = Vec::new();
            let dots = facts.chain_dot_breaks.get(&chain_id).unwrap_or(&empty);
            let broke_before = dots.iter().any(|&p| p <= c.lparen);
            let broke_after = dots.iter().any(|&p| p > c.lparen);
            let in_first_chunk = facts
                .chain_first_probe
                .get(&chain_id)
                .is_none_or(|&fp| c.lparen < fp);
            let desired = broke_after && !broke_before && !in_first_chunk;
            let anchored_in_chunk = self.abs_line.is_some_and(|ls| {
                facts
                    .chain_first_probe
                    .get(&chain_id)
                    .is_some_and(|&fp| ls >= fp)
            });
            if desired
                && !c.chain_wrapper_static
                && !anchored_in_chunk
                && self.same_body(c)
                && self.chain_bonus_from_walk != Some(chain_id)
            {
                self.total += cont;
            } else if !desired && c.chain_wrapper_static {
                self.total -= cont;
            }
        }
    }

    fn anchor_innermost_argument(&mut self, c: &Rc<ArgCtx>) -> bool {
        let i = self.line.i;
        let leaves = self.line.leaves;
        let config = self.line.config;
        let facts = self.line.facts;
        let leaf_line_indent = &self.line.history.line_indent;
        let chain_expansion_points = self.line.chain_expansion_points;
        let leaf = &leaves[i];
        if self.innermost_ctx && leaf.text != ")" {
            let elem_start = c
                .parent
                .as_ref()
                .and_then(|pp| {
                    (0..=c.lparen)
                        .rev()
                        .find(|&j| leaves[j].arg_probe_for == Some(pp.lparen))
                })
                .filter(|&es| {
                    facts.starts.get(es).copied().unwrap_or(false)
                        && facts.line_no.get(es) == facts.line_no.get(c.lparen)
                })
                .or_else(|| {
                    let chain_id = c.chain?;
                    let ls = facts.line_start_of.get(c.lparen).copied()?;
                    (leaves[ls].chain_probe_for == Some(chain_id)).then_some(ls)
                });
            if let Some(es) = elem_start {
                if let Some(&li) = leaf_line_indent.get(es) {
                    let chain_bonus = match c.chain {
                        Some(id)
                            if facts
                                .chain_first_probe
                                .get(&id)
                                .is_some_and(|&fp| c.lparen >= fp)
                                && chain_expansion_points.get(&id).is_some_and(|d| {
                                    d.iter().any(|&p| p > c.lparen)
                                        && !d.iter().any(|&p| p <= c.lparen)
                                }) =>
                        {
                            config.continuation_indent
                        }
                        _ => 0,
                    };
                    let argument_indent = if c.chain_line_capped {
                        0
                    } else {
                        config.continuation_indent
                    };
                    self.total = self
                        .total
                        .max((li + argument_indent + chain_bonus) as isize);
                    return true;
                }
            } else if leaf.arg_probe_for == Some(c.lparen) {
                let ancestor = |x: usize| {
                    let mut a = Some(self.root);
                    while let Some(cc) = a {
                        if cc.lparen == x {
                            return true;
                        }
                        a = cc.parent.as_ref();
                    }
                    false
                };
                let own_chunk = c.chain.and_then(|id| {
                    (0..=c.lparen)
                        .rev()
                        .find(|&j| leaves[j].chain_probe_for == Some(id))
                });
                let mut lp = c.lparen;
                let mut base = None;
                let mut skipped = false;
                let mut chain_skipped = false;
                for _ in 0..16 {
                    let ls = facts.line_start_of.get(lp).copied().unwrap_or(0);
                    if let (Some(id), Some(root)) = (c.chain, c.chain)
                        && leaves[ls].chain_probe_for == Some(id)
                        && own_chunk != Some(ls)
                        && root < ls
                    {
                        lp = root;
                        skipped = true;
                        chain_skipped = true;
                        continue;
                    }
                    match leaves.get(ls).and_then(|l| l.arg_probe_for) {
                        Some(x) if !ancestor(x) && x < lp => {
                            lp = x;
                            skipped = true;
                        }
                        _ => {
                            if skipped {
                                base = leaf_line_indent.get(ls).copied();
                            }
                            break;
                        }
                    }
                }
                if let Some(b) = base {
                    let mut c_chain_climbed: Option<usize> = None;
                    let mut chain_bonus = c
                        .chain
                        .filter(|id| facts.chain_dot_breaks.contains_key(id))
                        .map_or(0, |_| config.continuation_indent);
                    if chain_bonus == 0 {
                        let mut a = c.parent.as_ref();
                        while let Some(x) = a {
                            if let Some(id) = x.chain {
                                if facts
                                    .chain_first_probe
                                    .get(&id)
                                    .is_some_and(|&fp| x.lparen >= fp)
                                    && facts.chain_dot_breaks.get(&id).is_some_and(|d| {
                                        d.iter().any(|&p| p > x.lparen)
                                            && !d.iter().any(|&p| p <= x.lparen)
                                    })
                                {
                                    chain_bonus = config.continuation_indent;
                                    c_chain_climbed = Some(id);
                                }
                                break;
                            }
                            a = x.parent.as_ref();
                        }
                    }
                    let want = (b + config.continuation_indent + chain_bonus) as isize;
                    if chain_bonus > 0 {
                        self.chain_bonus_from_walk = c_chain_climbed;
                    }
                    if want < self.total || chain_skipped {
                        self.total = want;
                        return true;
                    }
                }
            }
        }

        false
    }
}
