use super::super::flatten::{Anchor, LeafEntry};
use super::EmitConfig;
use super::comments::comment_keeps_first_column;
use std::collections::{HashMap, HashSet};

pub(super) struct LineFacts {
    pub(super) starts: Vec<bool>,
    pub(super) line_start_of: Vec<usize>,
    pub(super) line_no: Vec<usize>,
    pub(super) own_break: HashSet<usize>,
    pub(super) own_break_first: HashMap<usize, usize>,
    pub(super) elem_firsts: HashMap<usize, Vec<usize>>,
    pub(super) rparen_break: HashSet<usize>,
    pub(super) cascade_rparen: HashSet<usize>,
    pub(super) zero_col_expand: HashSet<usize>,
    first_col_comments: Vec<usize>,
    pub(super) chain_dot_breaks: HashMap<usize, Vec<usize>>,
    pub(super) chain_first_probe: HashMap<usize, usize>,
    last_start: usize,
    cur_line: usize,
}

impl LineFacts {
    pub(super) fn broken_chain_line(&self, line_start: usize) -> bool {
        self.chain_dot_breaks
            .keys()
            .any(|root| self.line_start_of.get(*root).copied() == Some(line_start))
    }

    pub(super) fn new(leaf_count: usize) -> Self {
        Self {
            starts: Vec::with_capacity(leaf_count),
            line_start_of: Vec::with_capacity(leaf_count),
            line_no: Vec::with_capacity(leaf_count),
            own_break: HashSet::new(),
            own_break_first: HashMap::new(),
            elem_firsts: HashMap::new(),
            rparen_break: HashSet::new(),
            cascade_rparen: HashSet::new(),
            zero_col_expand: HashSet::new(),
            first_col_comments: Vec::new(),
            chain_dot_breaks: HashMap::new(),
            chain_first_probe: HashMap::new(),
            last_start: 0,
            cur_line: 0,
        }
    }

    pub(super) fn absorb(
        &mut self,
        index: usize,
        leaves: &[LeafEntry],
        config: &EmitConfig,
        src: &[u8],
    ) {
        let i = index;
        let leaf = &leaves[i];
        let s = i == 0 || leaf.white_space.line_feeds > 0;
        self.cur_line += leaf.white_space.line_feeds;
        self.starts.push(s);
        if s {
            self.last_start = i;
        }
        self.line_start_of.push(self.last_start);
        self.line_no.push(self.cur_line);
        if let Some(lp) = leaf.arg_probe_for {
            self.elem_firsts.entry(lp).or_default().push(i);
        }
        self.cur_line += leaf.text.bytes().filter(|&b| b == b'\n').count();
        if s && config.keep_first_column_comment
            && !leaf.member_bound_comment
            && (leaf.text.starts_with("//") || leaf.text.starts_with("/*"))
            && leaf
                .source_offset
                .is_some_and(|off| comment_keeps_first_column(src, off))
        {
            self.first_col_comments.push(i);
        }
        if s && i > 0 {
            if let Some(lp) = leaf.arg_probe_for {
                self.own_break.insert(lp);
                self.own_break_first.entry(lp).or_insert(i);
            }
            if let Some(ch) = leaf.chain_probe_for {
                self.chain_dot_breaks.entry(ch).or_default().push(i);
            }
        }
        if let Some(ch) = leaf.chain_probe_for
            && leaf.text == "."
            && leaves
                .get(i + 1)
                .is_some_and(|n| n.white_space.line_feeds > 0)
        {
            self.chain_dot_breaks.entry(ch).or_default().push(i);
        }
        if s && i > 0
            && leaf.anchor_self_exempt
            && let Some(Anchor::Arg(c)) = &leaf.anchor
        {
            self.rparen_break.insert(c.lparen);
            if self
                .first_col_comments
                .iter()
                .any(|&q| q > c.lparen && q < i)
            {
                self.zero_col_expand.insert(c.lparen);
            }
            let prev_open = self.line_start_of.get(i - 1).copied().unwrap_or(i - 1);
            if i > 0
                && leaves[prev_open].text == ")"
                && (prev_open == i - 1
                    || (prev_open + 1..i)
                        .any(|p| !matches!(leaves[p].text.as_str(), ")" | ";" | ",")))
            {
                self.cascade_rparen.insert(c.lparen);
            }
        }
        if let Some(ch) = leaf.chain_probe_for {
            self.chain_first_probe.entry(ch).or_insert(i);
        }
    }
}

pub(super) struct ChainFacts {
    pub(super) expansion_points: HashMap<usize, Vec<usize>>,
    pub(super) inner_breaks: HashMap<usize, Vec<usize>>,
    pub(super) expanded: HashSet<usize>,
}

impl ChainFacts {
    pub(super) fn new(leaves: &[LeafEntry], dot_breaks: &HashMap<usize, Vec<usize>>) -> Self {
        let mut chain_expansion_points = dot_breaks.clone();
        let mut chain_inner_breaks: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut chain_segment_positions: HashMap<usize, Vec<usize>> = HashMap::new();
        for (index, leaf) in leaves.iter().enumerate() {
            if let Some(chain) = leaf.chain_probe_for {
                chain_segment_positions
                    .entry(chain)
                    .or_default()
                    .push(index);
            }
        }
        let mut seen_chain_segment = HashSet::new();
        for (i, leaf) in leaves.iter().enumerate() {
            let Some(chain) = leaf.chain_probe_for else {
                continue;
            };
            if seen_chain_segment.insert(chain) {
                continue;
            }
            let end = leaf.span_end.min(leaves.len().saturating_sub(1));
            let segment_lparen = (i + 1..=end).find(|&j| leaves[j].text == "(");
            let segment_end = segment_lparen
                .and_then(|lparen| {
                    let mut depth = 0usize;
                    for (offset, entry) in leaves[lparen..=end].iter().enumerate() {
                        match entry.text.as_str() {
                            "(" => depth += 1,
                            ")" => {
                                depth = depth.saturating_sub(1);
                                if depth == 0 {
                                    return Some(lparen + offset);
                                }
                            }
                            _ => {}
                        }
                    }
                    None
                })
                .unwrap_or(end);
            let later_segment_count = chain_segment_positions.get(&chain).map_or(0, |positions| {
                positions.len() - positions.partition_point(|&position| position <= segment_end)
            });
            let single_argument = segment_lparen.is_some_and(|lparen| {
                let mut depth = 0usize;
                let mut has_top_level_comma = false;
                for leaf in &leaves[lparen..=segment_end] {
                    match leaf.text.as_str() {
                        "(" => depth += 1,
                        ")" => depth = depth.saturating_sub(1),
                        "," if depth == 1 => has_top_level_comma = true,
                        _ => {}
                    }
                }
                !has_top_level_comma
            });
            let argument_is_multi_argument_call = segment_lparen.is_some_and(|outer_lparen| {
                let nested_lparen = (outer_lparen + 1..segment_end).find(|&j| {
                    leaves[j].text == "("
                        && !leaves[outer_lparen + 1..j].iter().any(|entry| {
                            matches!(
                                entry.text.trim(),
                                "\"" | "+" | "-" | "*" | "/" | "%" | "?" | ":"
                            )
                        })
                });
                nested_lparen.is_some_and(|nested_lparen| {
                    let mut depth = 1usize;
                    let mut has_comma = false;
                    for (offset, entry) in leaves[nested_lparen..=segment_end].iter().enumerate() {
                        match entry.text.as_str() {
                            "(" => depth += 1,
                            ")" => {
                                depth = depth.saturating_sub(1);
                                if depth == 1 {
                                    return has_comma && nested_lparen + offset + 1 == segment_end;
                                }
                            }
                            "," if depth == 2 => has_comma = true,
                            _ => {}
                        }
                    }
                    false
                })
            });
            let has_lambda = leaves[i + 1..=segment_end]
                .iter()
                .any(|leaf| leaf.text.trim() == "->");
            if later_segment_count == 1
                && single_argument
                && argument_is_multi_argument_call
                && !has_lambda
                && let Some(j) =
                    (i + 1..=segment_end).find(|&j| leaves[j].white_space.contains_line_feeds())
            {
                chain_expansion_points.entry(chain).or_default().push(j);
                chain_inner_breaks.entry(chain).or_default().push(j);
            }
        }
        let expanded_chains: HashSet<usize> = chain_expansion_points.keys().copied().collect();
        Self {
            expansion_points: chain_expansion_points,
            inner_breaks: chain_inner_breaks,
            expanded: expanded_chains,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatter::flatten::flatten;
    use crate::formatter::ir::{Block, Spacing};

    fn config() -> EmitConfig {
        EmitConfig {
            indent_size: 4,
            continuation_indent: 8,
            use_tabs: false,
            keep_first_column_comment: true,
            align_multiline_ternary: false,
        }
    }

    #[test]
    fn incremental_facts_do_not_include_future_element_breaks() {
        let root = Block::node(vec![
            Block::leaf("f"),
            Block::leaf("x\ny").with_spacing(Spacing::new_line()),
            Block::leaf("z").with_spacing(Spacing::new_line()),
        ]);
        let mut leaves = Vec::new();
        flatten(&root, &mut leaves);
        leaves[1].arg_probe_for = Some(0);
        leaves[2].arg_probe_for = Some(0);
        let mut facts = LineFacts::new(leaves.len());
        facts.absorb(0, &leaves, &config(), &[]);
        assert_eq!(facts.starts, [true]);
        assert!(facts.own_break.is_empty());
        assert!(facts.elem_firsts.is_empty());

        facts.absorb(1, &leaves, &config(), &[]);
        assert_eq!(facts.own_break_first.get(&0), Some(&1));
        assert_eq!(facts.elem_firsts[&0], [1]);

        facts.absorb(2, &leaves, &config(), &[]);
        assert_eq!(facts.starts, [true, true, true]);
        assert_eq!(facts.line_no, [0, 1, 3]);
        assert_eq!(facts.line_start_of, [0, 1, 2]);
        assert_eq!(facts.own_break_first.get(&0), Some(&1));
        assert_eq!(facts.elem_firsts[&0], [1, 2]);
    }

    #[test]
    fn chain_snapshot_preserves_full_and_incremental_scan_distinction() {
        let root = Block::node(vec![
            Block::leaf("root"),
            Block::leaf("."),
            Block::leaf("call").with_spacing(Spacing::new_line()),
        ]);
        let mut leaves = Vec::new();
        flatten(&root, &mut leaves);
        leaves[0].chain_probe_for = Some(0);
        leaves[1].chain_probe_for = Some(0);
        let mut facts = LineFacts::new(leaves.len());
        let scan_snapshot = ChainFacts::new(&leaves, &facts.chain_dot_breaks);
        assert!(!facts.broken_chain_line(0));
        for i in 0..leaves.len() {
            facts.absorb(i, &leaves, &config(), &[]);
        }
        assert_eq!(facts.chain_dot_breaks[&0], [1]);
        assert!(facts.broken_chain_line(0));
        assert!(!facts.broken_chain_line(1));
        assert!(scan_snapshot.expanded.is_empty());
        let full_snapshot = ChainFacts::new(&leaves, &facts.chain_dot_breaks);
        assert!(full_snapshot.expanded.contains(&0));
        assert_eq!(full_snapshot.expansion_points[&0], [1]);
    }
}
