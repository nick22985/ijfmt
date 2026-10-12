use super::{Anchor, LeafEntry, WrapProcessor};
use super::{HashMap, Wrap, WrapId, WrapState, WrapType};

impl WrapProcessor {
    pub(super) fn get_wrap_to_use(
        &self,
        i: usize,
        leaves: &[LeafEntry],
        wraps: &HashMap<WrapId, Wrap>,
        wrap_states: &HashMap<WrapId, WrapState>,
        wrap_candidate: Option<(usize, WrapId)>,
        positional: &std::collections::HashSet<WrapId>,
    ) -> Option<WrapId> {
        let leaf_wraps = &leaves[i].wraps;
        if leaf_wraps.is_empty() {
            return None;
        }

        if let Some((cand_idx, _cand_wid)) = wrap_candidate
            && cand_idx == i
        {
            return leaf_wraps.first().copied();
        }

        for &wid in leaf_wraps {
            let wrap_def = match wraps.get(&wid) {
                Some(w) => w,
                None => continue,
            };

            if !self.is_suitable(wid, i, wrap_def, wrap_states, leaves, positional) {
                continue;
            }

            if let Some(ws) = wrap_states.get(&wid)
                && ws.active
            {
                return Some(wid);
            }

            if wrap_def.wrap_type == WrapType::Always {
                return Some(wid);
            }

            if matches!(
                wrap_def.wrap_type,
                WrapType::Normal | WrapType::ChopIfNeeded
            ) && self.line_over(i, leaves)
            {
                return Some(wid);
            }
        }

        None
    }

    pub(super) fn is_suitable(
        &self,
        wid: WrapId,
        i: usize,
        wrap_def: &Wrap,
        wrap_states: &HashMap<WrapId, WrapState>,
        leaves: &[LeafEntry],
        positional: &std::collections::HashSet<WrapId>,
    ) -> bool {
        if let Some(ws) = wrap_states.get(&wid)
            && let Some(first_offset) = ws.first_offset
            && first_offset < i
        {
            return true;
        }

        if wrap_def.wrap_first_element {
            return true;
        }

        if positional.contains(&wid) && wrap_def.wrap_type == WrapType::Normal {
            return self.position_after_wrapping_is_suitable(i, leaves);
        }
        false
    }

    pub(super) fn position_after_wrapping_is_suitable(
        &self,
        i: usize,
        leaves: &[LeafEntry],
    ) -> bool {
        if leaves[i].white_space.contains_line_feeds() {
            return true;
        }
        let start_col_now = {
            let end = self.compute_end_column(i, leaves);
            let text = &leaves[i].text;
            let last = text.rfind('\n').map(|p| &text[p + 1..]).unwrap_or(text);
            end.saturating_sub(self.text_width(last))
        };
        let mut start_col_after = self.compute_indent_width(&leaves[i].indent_stack);
        if let Some((vf, gate)) = leaves[i].break_bonus {
            let gate_open =
                gate.is_none_or(|g| g == i || leaves[g].white_space.contains_line_feeds());
            if gate_open && (vf == i || leaves[vf].white_space.contains_line_feeds()) {
                start_col_after += self.continuation_indent;
            }
        }
        if let Some(Anchor::Arg(ctx)) = &leaves[i].anchor {
            let mut cur = ctx;
            loop {
                if (cur.collapsed || cur.channel)
                    && !cur.elem_has_cont
                    && leaves[i].arg_probe_for == Some(cur.lparen)
                {
                    start_col_after += self.continuation_indent;
                }
                match &cur.parent {
                    Some(p) => cur = p,
                    None => break,
                }
            }
        }
        if let Some(lp) = leaves[i].paren_cwf
            && lp < i
        {
            let ls = self.paren_ancestor_line_start(lp, leaves);
            let base = self.leaf_start_column(ls, leaves);
            start_col_after = base + self.continuation_indent;
        }
        if i > 0
            && matches!(leaves[i - 1].text.trim(), "?" | ":")
            && leaves[i - 1].white_space.contains_line_feeds()
        {
            start_col_after = if leaves[i - 1].text.trim() == "?"
                && leaves[i].tern_bin == Some(i)
                && leaves[i].tern_branch.is_some()
            {
                self.leaf_start_column(i - 1, leaves)
            } else {
                self.leaf_start_column(i - 1, leaves) + self.continuation_indent
            };
        }
        if i > 1 && leaves[i - 1].text.trim() == "(" {
            let mut ls = 0usize;
            for j in (0..i).rev() {
                if j == 0 || leaves[j].white_space.contains_line_feeds() {
                    ls = j;
                    break;
                }
            }
            if ls < i - 1
                && leaves[ls].text.trim_start().starts_with('.')
                && leaves[ls].span_end >= i
            {
                let base = self.leaf_start_column(ls, leaves) + self.continuation_indent;
                if base > start_col_after {
                    start_col_after = base;
                }
            }
        }
        if leaves[i].anchor.is_none()
            && leaves[i].bin_op.is_none()
            && let Some(binary_first) = leaves[i].bin_first
            && binary_first < i
        {
            let ancestor = self.paren_ancestor_line_start(binary_first, leaves);
            start_col_after = self.leaf_start_column(ancestor, leaves) + self.continuation_indent;
        }
        if let Some(lparen) = leaves[i].paren_cwf
            && lparen > 0
            && leaves[i].wraps.len() >= 2
            && leaves[lparen - 1].white_space.contains_line_feeds()
            && matches!(leaves[lparen - 1].text.trim(), "&&" | "||")
        {
            start_col_after = self.leaf_start_column(lparen - 1, leaves);
        }
        start_col_now > start_col_after
    }

    pub(super) fn is_candidate(
        &self,
        wid: WrapId,
        i: usize,
        wrap_def: &Wrap,
        wrap_states: &HashMap<WrapId, WrapState>,
        leaves: &[LeafEntry],
        positional: &std::collections::HashSet<WrapId>,
    ) -> bool {
        matches!(
            wrap_def.wrap_type,
            WrapType::Normal | WrapType::ChopIfNeeded
        ) && self.is_suitable(wid, i, wrap_def, wrap_states, leaves, positional)
    }

    pub(super) fn line_over(&self, i: usize, leaves: &[LeafEntry]) -> bool {
        let end_col = self.compute_end_column(i, leaves);
        end_col > self.right_margin
    }

    pub(super) fn candidate_break_still_over(
        &self,
        cand_idx: usize,
        leaves: &mut [LeafEntry],
    ) -> bool {
        let ctx = match &leaves[cand_idx].anchor {
            Some(Anchor::Arg(ctx))
                if leaves[cand_idx].bin_first.is_none()
                    && !leaves[cand_idx].encl_chain.is_empty()
                    && leaves[cand_idx]
                        .text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
                    && leaves[cand_idx].arg_probe_for == Some(ctx.lparen) =>
            {
                ctx.clone()
            }
            _ => return false,
        };
        if cand_idx <= ctx.lparen || leaves[cand_idx].white_space.contains_line_feeds() {
            return false;
        }

        let mut depth = 0usize;
        let mut close = None;
        let mut has_top_level_comma = false;
        let mut has_top_level_comma_after_candidate = false;
        let mut ternary_signs = 0usize;
        for (index, leaf) in leaves.iter().enumerate().skip(ctx.lparen) {
            match leaf.text.as_str() {
                "(" => depth += 1,
                ")" => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        close = Some(index);
                        break;
                    }
                }
                "," if depth == 1 => {
                    has_top_level_comma = true;
                    has_top_level_comma_after_candidate |= index > cand_idx;
                }
                "?" | ":" if index > cand_idx => ternary_signs += 1,
                _ => {}
            }
        }
        let Some(close) = close else {
            return false;
        };
        let chain_segments: Vec<usize> = (cand_idx..=close)
            .filter(|&index| {
                leaves[index].chain_probe_for == Some(cand_idx)
                    && leaves[index].text.trim_start().starts_with('.')
            })
            .collect();
        let two_stage_accessor_chain = chain_segments.len() == 2
            && leaves
                .get(chain_segments[0] + 1..=chain_segments[0] + 2)
                .is_some_and(|pair| pair[0].text == "(" && pair[1].text == ")")
            && leaves
                .get(chain_segments[1] + 1..=chain_segments[1] + 2)
                .is_some_and(|pair| pair[0].text == "(" && pair[1].text != ")");
        let three_stage_accessor_nested_terminal_chain = chain_segments.len() == 3
            && leaves
                .get(chain_segments[0] + 1..=chain_segments[0] + 2)
                .is_some_and(|pair| pair[0].text == "(" && pair[1].text == ")")
            && leaves
                .get(chain_segments[1] + 1..=chain_segments[1] + 2)
                .is_some_and(|pair| pair[0].text == "(" && pair[1].text != ")")
            && leaves
                .get(chain_segments[2] + 1..=chain_segments[2] + 2)
                .is_some_and(|pair| pair[0].text == "(" && pair[1].text == ")");
        let expression_lambda_body = leaves[cand_idx.saturating_sub(24)..cand_idx]
            .iter()
            .rev()
            .take_while(|leaf| !matches!(leaf.text.trim(), ";" | "{" | "}"))
            .any(|leaf| leaf.text.trim_end().ends_with("->"));
        let candidate_starts_lowercase = leaves[cand_idx]
            .text
            .chars()
            .next()
            .is_some_and(char::is_lowercase);
        let candidate_starts_uppercase = leaves[cand_idx]
            .text
            .chars()
            .next()
            .is_some_and(char::is_uppercase);
        let body_chain_candidate = leaves[cand_idx].wraps.len() == 1
            && !has_top_level_comma
            && ((ctx.body.is_some() && candidate_starts_lowercase && two_stage_accessor_chain)
                || (expression_lambda_body
                    && candidate_starts_uppercase
                    && three_stage_accessor_nested_terminal_chain));
        let last_argument_ternary = leaves[cand_idx].wraps.len() >= 2
            && ternary_signs >= 4
            && has_top_level_comma
            && ctx.seg_first.is_some()
            && !has_top_level_comma_after_candidate;
        if (!body_chain_candidate && !last_argument_ternary)
            || (cand_idx + 1..=close).any(|index| leaves[index].white_space.contains_line_feeds())
        {
            return false;
        }

        let saved = leaves[cand_idx].white_space.clone();
        let saved_chain_breaks = self.chain_src_break.borrow().clone();
        leaves[cand_idx].white_space.ensure_line_feed();
        let end_col = self.compute_end_column(close, leaves);
        let missing_candidate_oracle = self
            .indent_oracle
            .as_ref()
            .map(|oracle| &oracle.full)
            .is_some_and(|oracle| oracle.get(cand_idx).is_none_or(|indent| indent.is_none()));
        let still_over = end_col
            + if missing_candidate_oracle {
                self.continuation_indent
            } else {
                0
            }
            > self.right_margin;
        leaves[cand_idx].white_space = saved;
        *self.chain_src_break.borrow_mut() = saved_chain_breaks;
        still_over
    }

    pub(super) fn array_element_fits_after_break(
        &self,
        cand_idx: usize,
        leaves: &[LeafEntry],
    ) -> bool {
        let Some(element) = leaves[cand_idx].arr_elem else {
            return false;
        };
        let mut end = cand_idx;
        while end + 1 < leaves.len() && leaves[end + 1].arr_elem == Some(element) {
            end += 1;
        }
        if leaves[cand_idx + 1..=end]
            .iter()
            .any(|leaf| leaf.white_space.contains_line_feeds())
        {
            return false;
        }
        let mut width = self.compute_indent_width(&leaves[cand_idx].indent_stack)
            + self.text_width(&leaves[cand_idx].text);
        for leaf in &leaves[cand_idx + 1..=end] {
            width += leaf.white_space.spaces + self.text_width(&leaf.text);
        }
        width <= self.right_margin
    }
}
