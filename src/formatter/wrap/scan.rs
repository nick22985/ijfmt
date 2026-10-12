use super::{
    Anchor, HashMap, HashSet, LeafEntry, Wrap, WrapId, WrapProcessor, WrapState, WrapType,
    is_child_of,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExistingBreak {
    Absent,
    Original,
    Generated,
}

enum ScanStep {
    Advance,
    Rewind(usize),
}

pub(super) struct WrapScan<'a> {
    processor: &'a WrapProcessor,
    wraps: &'a HashMap<WrapId, Wrap>,
    positional: &'a HashSet<WrapId>,
    extra_parents: &'a HashMap<WrapId, HashSet<WrapId>>,
    initial_feeds: Vec<bool>,
    wrap_states: HashMap<WrapId, WrapState>,
    wrap_candidate: Option<(usize, WrapId)>,
    first_wrapped_on_line: Option<usize>,
    relation_ignores: HashSet<(WrapId, WrapId, usize)>,
    debug_wrap: bool,
    debug_column: bool,
}

impl<'a> WrapScan<'a> {
    pub(super) fn new(
        processor: &'a WrapProcessor,
        leaves: &[LeafEntry],
        wraps: &'a HashMap<WrapId, Wrap>,
        positional: &'a HashSet<WrapId>,
        extra_parents: &'a HashMap<WrapId, HashSet<WrapId>>,
    ) -> Self {
        Self {
            processor,
            wraps,
            positional,
            extra_parents,
            initial_feeds: leaves
                .iter()
                .map(|leaf| leaf.white_space.contains_line_feeds())
                .collect(),
            wrap_states: HashMap::new(),
            wrap_candidate: None,
            first_wrapped_on_line: None,
            relation_ignores: HashSet::new(),
            debug_wrap: std::env::var("IJFMT_WRAP_DBG").is_ok(),
            debug_column: std::env::var("IJFMT_COL_DBG").is_ok(),
        }
    }

    pub(super) fn run(mut self, leaves: &mut [LeafEntry]) {
        self.trace_input(leaves);
        let max_iterations = leaves.len().saturating_mul(40).max(4096);
        let mut i = 0;
        for _ in 0..max_iterations {
            if i >= leaves.len() {
                break;
            }
            i = match self.scan_leaf(i, leaves) {
                ScanStep::Advance => i + 1,
                ScanStep::Rewind(index) => index,
            };
        }
    }

    fn scan_leaf(&mut self, i: usize, leaves: &mut [LeafEntry]) -> ScanStep {
        if i == 0 {
            self.record_first_offsets(i, leaves);
            return ScanStep::Advance;
        }
        let existing = self.prepare_line_break(i, leaves);
        self.record_first_offsets(i, leaves);
        let selected_wrap = self.processor.get_wrap_to_use(
            i,
            leaves,
            self.wraps,
            &self.wrap_states,
            self.wrap_candidate,
            self.positional,
        );
        if selected_wrap.is_some() || existing == ExistingBreak::Original {
            if let Some(index) = self.apply_wrap(i, leaves, selected_wrap, existing) {
                return ScanStep::Rewind(index);
            }
        } else {
            self.consider_candidates(i, leaves);
        }
        match self.overflow_rewind(i, leaves) {
            Some(index) => ScanStep::Rewind(index),
            None => ScanStep::Advance,
        }
    }

    fn record_first_offsets(&mut self, i: usize, leaves: &[LeafEntry]) {
        for &wid in &leaves[i].wraps {
            self.wrap_states
                .entry(wid)
                .or_default()
                .first_offset
                .get_or_insert(i);
        }
    }

    fn prepare_line_break(&mut self, i: usize, leaves: &mut [LeafEntry]) -> ExistingBreak {
        if !leaves[i].white_space.contains_line_feeds() {
            return ExistingBreak::Absent;
        }
        self.first_wrapped_on_line = None;
        if self.initial_feeds[i] {
            self.wrap_candidate = None;
            ExistingBreak::Original
        } else {
            leaves[i].white_space.remove_line_feeds();
            ExistingBreak::Generated
        }
    }

    fn apply_wrap(
        &mut self,
        i: usize,
        leaves: &mut [LeafEntry],
        selected_wrap: Option<WrapId>,
        existing: ExistingBreak,
    ) -> Option<usize> {
        if self.debug_wrap {
            eprintln!(
                "[DBG fire] i={} text={:?} selected={:?} present={} cand={:?}",
                i,
                leaves[i].text,
                selected_wrap,
                existing == ExistingBreak::Original,
                self.wrap_candidate
            );
        }
        if let Some(index) = self.preferred_candidate_rewind(i, leaves, selected_wrap, existing) {
            return Some(index);
        }
        if let Some(index) = self.activate_chop(i, selected_wrap) {
            return Some(index);
        }
        if let Some(index) = self.insert_line_break(i, leaves, selected_wrap, existing) {
            return Some(index);
        }
        self.wrap_candidate = None;
        None
    }

    fn preferred_candidate_rewind(
        &self,
        i: usize,
        leaves: &[LeafEntry],
        selected_wrap: Option<WrapId>,
        existing: ExistingBreak,
    ) -> Option<usize> {
        if existing != ExistingBreak::Original
            && let Some(selected_wid) = selected_wrap
            && let Some((cand_idx, cand_wid)) = self.wrap_candidate
        {
            let cand_wid = leaves[cand_idx].wraps.first().copied().unwrap_or(cand_wid);
            let nested_detached_argument_near_margin = i > 0
                && leaves[i].text.trim_start().starts_with('.')
                && self.processor.compute_end_column(i - 1, leaves)
                    > self
                        .processor
                        .right_margin
                        .saturating_sub(self.processor.continuation_indent)
                && matches!(
                    &leaves[cand_idx].anchor,
                    Some(Anchor::Arg(ctx))
                        if ctx.parent.as_ref().is_some_and(|parent| {
                            parent.ignore_parent_wraps
                        })
                );
            if self.debug_wrap {
                eprintln!(
                    "[DBG 4a] i={} sel={:?} cand_idx={} cand_wid={:?} can_replace={}",
                    i,
                    selected_wid,
                    cand_idx,
                    cand_wid,
                    self.can_replace_candidate(selected_wid, i, cand_wid)
                );
            }
            if cand_idx != i
                && (nested_detached_argument_near_margin
                    || !self.can_replace_candidate(selected_wid, i, cand_wid))
            {
                return Some(cand_idx);
            }
        }
        None
    }

    fn activate_chop(&mut self, i: usize, selected_wrap: Option<WrapId>) -> Option<usize> {
        if let Some(selected_wid) = selected_wrap {
            let wrap_def = &self.wraps[&selected_wid];
            if wrap_def.wrap_type == WrapType::ChopIfNeeded {
                let ws = self.wrap_states.entry(selected_wid).or_default();
                if let Some(chop_start) = ws.chop_start
                    && !ws.active
                    && chop_start < i
                {
                    ws.active = true;
                    self.wrap_candidate = None;
                    self.first_wrapped_on_line = None;
                    return Some(chop_start);
                }
                let ws = self.wrap_states.entry(selected_wid).or_default();
                if !ws.active {
                    ws.active = true;
                }
            }
        }
        None
    }

    fn insert_line_break(
        &mut self,
        i: usize,
        leaves: &mut [LeafEntry],
        selected_wrap: Option<WrapId>,
        existing: ExistingBreak,
    ) -> Option<usize> {
        if existing != ExistingBreak::Original {
            leaves[i].white_space.ensure_line_feed();
            if existing == ExistingBreak::Absent {
                if let (Some(selected_wid), Some(fwol)) =
                    (selected_wrap, self.first_wrapped_on_line)
                    && let Some(&fwol_wid) = leaves[fwol].wraps.first()
                {
                    let direct_unary_chain_argument =
                        self.processor.direct_unary_chain_argument(i, fwol, leaves);
                    let body_argument_chain = self.processor.body_argument_chain(i, fwol, leaves);
                    if !direct_unary_chain_argument
                        && !body_argument_chain
                        && self.is_child_of_at(selected_wid, fwol_wid, i)
                    {
                        self.relation_ignores.insert((selected_wid, fwol_wid, i));
                        return Some(fwol);
                    }
                }
                self.first_wrapped_on_line = Some(i);
            }
        }
        None
    }

    fn consider_candidates(&mut self, i: usize, leaves: &mut [LeafEntry]) {
        let candidate_needs_inner_wrap = self.wrap_candidate.is_some_and(|(cand_idx, _)| {
            self.processor.candidate_break_still_over(cand_idx, leaves)
        });
        for &wid in &leaves[i].wraps {
            if let Some(wrap_def) = self.wraps.get(&wid) {
                let replaceable = self.candidate_is_replaceable(
                    i,
                    wid,
                    wrap_def,
                    leaves,
                    candidate_needs_inner_wrap,
                );

                if self.debug_wrap {
                    eprintln!(
                        "[DBG cand?] i={} text={:?} wid={:?} is_cand={} replaceable={}",
                        i,
                        leaves[i].text,
                        wid,
                        self.processor.is_candidate(
                            wid,
                            i,
                            wrap_def,
                            &self.wrap_states,
                            leaves,
                            self.positional
                        ),
                        replaceable
                    );
                }
                if self.processor.is_candidate(
                    wid,
                    i,
                    wrap_def,
                    &self.wrap_states,
                    leaves,
                    self.positional,
                ) && replaceable
                {
                    self.wrap_candidate = Some((i, wid));
                }
                if wrap_def.wrap_type == WrapType::ChopIfNeeded
                    && self.processor.is_suitable(
                        wid,
                        i,
                        wrap_def,
                        &self.wrap_states,
                        leaves,
                        self.positional,
                    )
                {
                    let ws = self.wrap_states.entry(wid).or_default();
                    if ws.chop_start.is_none() {
                        ws.chop_start = Some(i);
                    }
                }
            }
        }
    }

    fn candidate_is_replaceable(
        &self,
        i: usize,
        wid: WrapId,
        wrap_def: &Wrap,
        leaves: &[LeafEntry],
        candidate_needs_inner_wrap: bool,
    ) -> bool {
        match self.wrap_candidate {
            None => true,
            Some((cand_idx, _))
                if wrap_def.cand_ignore_parents
                    && cand_idx + 2 == i
                    && leaves[i - 1].text.trim() == "("
                    && !leaves[cand_idx].text.trim_start().starts_with('.') =>
            {
                true
            }
            Some((cand_idx, cand_wid)) => {
                let cand_wid = leaves[cand_idx].wraps.first().copied().unwrap_or(cand_wid);
                let nested_assignment_binary = cand_idx < i
                    && self.positional.contains(&cand_wid)
                    && self.positional.contains(&wid)
                    && leaves[cand_idx].text == "("
                    && leaves[i].text == "("
                    && leaves[i]
                        .wraps
                        .first()
                        .is_some_and(|&outer| outer != wid && !self.positional.contains(&outer))
                    && leaves[cand_idx].paren_cwf.is_some()
                    && leaves[cand_idx..i]
                        .iter()
                        .filter(|leaf| leaf.text.trim() == "=")
                        .take(2)
                        .count()
                        == 2
                    && self.is_child_of_at(wid, cand_wid, i);
                let retain_earlier_array_chain_dot = wid == cand_wid
                    && leaves[cand_idx].text.trim_start().starts_with('.')
                    && leaves[i].text.trim_start().starts_with('.')
                    && self
                        .processor
                        .array_element_fits_after_break(cand_idx, leaves);
                let lambda_body_child = i == cand_idx + 2
                    && leaves[cand_idx + 1].text.trim() == "->"
                    && leaves[i..]
                        .iter()
                        .filter(|leaf| leaf.wraps.contains(&wid) && leaf.text.trim() == "+")
                        .take(6)
                        .count()
                        == 6
                    && self.is_child_of_at(wid, cand_wid, i);
                let unresolved_inner_child = candidate_needs_inner_wrap
                    && ((leaves[i].text.trim_start().starts_with('.')
                        && leaves[i].chain_probe_for.is_some())
                        || matches!(leaves[i].text.trim(), "?" | ":"))
                    && self.is_child_of_at(wid, cand_wid, i);
                !retain_earlier_array_chain_dot
                    && (nested_assignment_binary
                        || lambda_body_child
                        || unresolved_inner_child
                        || self.can_replace_candidate(wid, i, cand_wid))
            }
        }
    }

    fn overflow_rewind(&self, i: usize, leaves: &[LeafEntry]) -> Option<usize> {
        if !leaves[i].white_space.contains_line_feeds()
            && let Some((cand_idx, cand_wid)) = self.wrap_candidate
        {
            if self.debug_column {
                eprintln!(
                    "[DBG col] i={} text={:?} end_col={} cand={}",
                    i,
                    leaves[i].text,
                    self.processor.compute_end_column(i, leaves),
                    cand_idx
                );
            }
            let near_margin_body_chain = self.processor.near_margin_body_chain(i, cand_idx, leaves);
            let later_binary_member = self.processor.has_later_binary_member(
                i,
                cand_idx,
                cand_wid,
                leaves,
                &self.initial_feeds,
            );
            if cand_idx != i
                && (self.processor.line_over(i, leaves) || near_margin_body_chain)
                && !later_binary_member
            {
                if self.debug_wrap {
                    eprintln!(
                        "[DBG overflow] i={} text={:?} -> cand={}",
                        i, leaves[i].text, cand_idx
                    );
                }
                return Some(cand_idx);
            }
        }
        None
    }

    fn trace_input(&self, leaves: &[LeafEntry]) {
        if self.debug_wrap {
            let mut ps: Vec<_> = self.extra_parents.iter().collect();
            ps.sort_by_key(|(k, _)| k.0);
            for (k, v) in ps {
                eprintln!("[DBG parent] {:?} -> {:?}", k, v);
            }
            for (idx, leaf) in leaves.iter().enumerate() {
                eprintln!(
                    "[DBG leaf] {} text={:?} wraps={:?} ws_lf={} ws_sp={}",
                    idx,
                    leaf.text,
                    leaf.wraps,
                    leaf.white_space.line_feeds,
                    leaf.white_space.spaces
                );
            }
        }
    }

    fn can_replace_candidate(&self, wid: WrapId, i: usize, cand_wid: WrapId) -> bool {
        if let Some(state) = self.wrap_states.get(&wid)
            && state.active
            && let Some(wrap) = self.wraps.get(&wid)
            && matches!(wrap.wrap_type, WrapType::ChopIfNeeded | WrapType::Always)
        {
            return true;
        }
        wid == cand_wid || !self.is_child_of_at(wid, cand_wid, i)
    }

    fn is_child_of_at(&self, child: WrapId, parent: WrapId, i: usize) -> bool {
        is_child_of(
            child,
            parent,
            self.wraps,
            self.extra_parents,
            &self.relation_ignores,
            i,
        )
    }
}
