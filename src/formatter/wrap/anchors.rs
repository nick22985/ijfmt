use super::{Anchor, ArgCtx, IndentType, LeafEntry, WrapProcessor, is_binary_op_text};

impl WrapProcessor {
    pub(super) fn leaf_start_column(&self, j: usize, leaves: &[LeafEntry]) -> usize {
        let end = self.compute_end_column(j, leaves);
        let text = &leaves[j].text;
        let last = text.rfind('\n').map(|p| &text[p + 1..]).unwrap_or(text);
        end.saturating_sub(self.text_width(last))
    }

    pub(super) fn paren_ancestor_line_start(&self, at: usize, leaves: &[LeafEntry]) -> usize {
        let line_start = |k: usize| {
            let mut ls = 0;
            for j in (0..=k).rev() {
                if j == 0 || leaves[j].white_space.contains_line_feeds() {
                    ls = j;
                    break;
                }
            }
            ls
        };
        let mut ls = line_start(at);
        for _ in 0..64 {
            let covers = leaves.get(ls).is_none_or(|l| {
                l.span_end >= at
                    && l.text != "?"
                    && !(l.bin_first.is_some() && is_binary_op_text(&l.text))
            });
            if covers {
                break;
            }
            match ls.checked_sub(1).map(line_start) {
                Some(prev)
                    if prev < ls
                        && self.leaf_start_column(prev, leaves)
                            <= self.leaf_start_column(ls, leaves) =>
                {
                    ls = prev;
                }
                _ => break,
            }
        }
        ls
    }

    pub(super) fn channel1_guard(&self, chain_root: usize, leaves: &[LeafEntry]) -> bool {
        if chain_root == 0 || leaves[chain_root].white_space.contains_line_feeds() {
            return true;
        }
        if leaves[chain_root - 1].text.trim_end().ends_with("->") {
            return true;
        }
        let mut ls = 0;
        for j in (0..chain_root).rev() {
            if j == 0 || leaves[j].white_space.contains_line_feeds() {
                ls = j;
                break;
            }
        }
        let ls_leaf = &leaves[ls];
        if !ls_leaf
            .indent_stack
            .iter()
            .any(|x| matches!(x, IndentType::Continuation))
        {
            return true;
        }
        if ls_leaf.text.as_str() == ":" || ls_leaf.text.as_str() == "(" {
            return true;
        }
        if matches!(&ls_leaf.anchor, Some(Anchor::Arg(_)))
            && ls_leaf.bin_op.is_none()
            && !matches!(
                ls_leaf.text.trim(),
                "+" | "-"
                    | "*"
                    | "/"
                    | "%"
                    | "&&"
                    | "||"
                    | "&"
                    | "|"
                    | "^"
                    | "<<"
                    | ">>"
                    | ">>>"
                    | "=="
                    | "!="
                    | "<"
                    | ">"
                    | "<="
                    | ">="
                    | "instanceof"
            )
        {
            return true;
        }
        false
    }

    pub(super) fn enforce_deduction(&self, ls: usize, leaves: &[LeafEntry]) -> usize {
        let inner = match &leaves[ls].anchor {
            Some(Anchor::Arg(c)) => c.clone(),
            _ => return 0,
        };
        if leaves[ls].arg_probe_for != Some(inner.lparen) {
            return 0;
        }
        if !inner.elem_has_cont {
            return 0;
        }
        let cancelled = inner.seg_cancel
            && inner.chain.is_some_and(|cr| {
                let fp = leaves.iter().position(|l| l.chain_probe_for == Some(cr));
                if fp.is_none_or(|fp| inner.lparen < fp) {
                    return false;
                }
                let mut after = false;
                let mut before = false;
                for (j, l) in leaves.iter().enumerate() {
                    if l.chain_probe_for == Some(cr) && l.white_space.contains_line_feeds() {
                        if j > inner.lparen {
                            after = true;
                        } else {
                            before = true;
                        }
                    }
                }
                after && !before
            });
        let mut ded = 0usize;
        let mut cur = inner.parent.clone();
        while let Some(c) = cur {
            if c.smart
                && c.elem_has_cont
                && c.lparen < ls
                && let Some(p) = (c.lparen + 1..ls)
                    .rev()
                    .find(|&j| leaves[j].arg_probe_for == Some(c.lparen))
                && !leaves[p].white_space.contains_line_feeds()
                && !(p + 1..ls).any(|j| leaves[j].white_space.contains_line_feeds())
            {
                let earlier = (c.lparen + 1..p).any(|j| {
                    leaves[j].arg_probe_for == Some(c.lparen)
                        && leaves[j].white_space.contains_line_feeds()
                });
                if !earlier {
                    ded += self.continuation_indent;
                }
            }
            cur = c.parent.clone();
        }
        if inner.chain_wrapper_static
            && let Some(sf) = inner.seg_first
            && sf < ls
            && !leaves[sf].white_space.contains_line_feeds()
            && !cancelled
        {
            ded += self.continuation_indent;
        }
        ded
    }

    pub(super) fn body_base_leaf(
        &self,
        cands: &[usize],
        encl: Option<&std::rc::Rc<ArgCtx>>,
        leaves: &[LeafEntry],
    ) -> Option<usize> {
        let encloses = |lp: usize| {
            let mut cur = encl;
            while let Some(c) = cur {
                if c.lparen == lp {
                    return true;
                }
                cur = c.parent.as_ref();
            }
            false
        };
        let mut b = cands.first().copied();
        let mut climb = encl.cloned();
        while let Some(a) = b {
            if a == 0 || leaves[a].white_space.contains_line_feeds() {
                break;
            }
            let mut ls = a;
            while ls > 0 && !leaves[ls].white_space.contains_line_feeds() {
                ls -= 1;
            }
            match leaves.get(ls).and_then(|l| l.arg_probe_for) {
                Some(lp) if !encloses(lp) && lp < a => {
                    b = Some(lp);
                    continue;
                }
                _ => {}
            }
            if leaves.get(ls).is_none_or(|l| l.span_end >= a) {
                break;
            }
            match climb.as_ref().filter(|c| c.lparen < a && !c.smart) {
                Some(c) => {
                    let lp = c.lparen;
                    let parent = c.parent.clone();
                    climb = parent;
                    b = Some(lp);
                }
                None => {
                    if let Some(c) = climb.as_ref().filter(|c| c.lparen < a && c.smart) {
                        if let Some(e) = (0..a).rev().find(|&j| {
                            leaves[j].arg_probe_for == Some(c.lparen)
                                && (j == 0 || leaves[j].white_space.contains_line_feeds())
                        }) {
                            b = Some(e);
                            break;
                        }
                        let lp = c.lparen;
                        let parent = c.parent.clone();
                        let elem_of =
                            |x: usize| (0..=x).rev().find(|&j| leaves[j].arg_probe_for == Some(lp));
                        if ls > lp && elem_of(ls) != elem_of(a) {
                            climb = parent;
                            b = Some(lp);
                            continue;
                        }
                    }
                    break;
                }
            }
        }
        b
    }

    pub(super) fn anchor_line_indent(
        &self,
        a: usize,
        line_start: usize,
        leaves: &[LeafEntry],
        depth: usize,
    ) -> usize {
        let mut als = 0;
        for j in (0..=a).rev() {
            if j == 0 || leaves[j].white_space.contains_line_feeds() {
                als = j;
                break;
            }
        }
        if als == line_start {
            0
        } else {
            self.compute_end_column_inner(als, leaves, depth + 1)
                .saturating_sub({
                    let t = &leaves[als].text;
                    let last = t.rfind('\n').map(|p| &t[p + 1..]).unwrap_or(t);
                    self.text_width(last)
                })
        }
    }
}
