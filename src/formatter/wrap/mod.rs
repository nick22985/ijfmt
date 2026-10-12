mod anchors;
mod backtracking;
mod candidates;
mod columns;
mod feedback;
mod relations;
mod scan;

pub use relations::collect_wrap_parents;
use relations::is_child_of;

use std::collections::{HashMap, HashSet};

use super::ir::*;

type ChainSourceBreaks = HashMap<usize, Vec<(usize, bool)>>;
use super::flatten::{Anchor, ArgCtx, LeafEntry};

#[derive(Debug, Default)]
struct WrapState {
    active: bool,
    chop_start: Option<usize>,
    first_offset: Option<usize>,
}

pub(super) struct IndentOracle {
    full: Vec<Option<usize>>,
    scan: Vec<Option<usize>>,
}

impl IndentOracle {
    pub fn from_emission(full: Vec<usize>, scan: Vec<usize>, started: &[bool]) -> Self {
        let for_line_starts = |indents: Vec<usize>| {
            started
                .iter()
                .zip(indents)
                .map(|(&starts_line, indent)| starts_line.then_some(indent))
                .collect()
        };
        Self {
            full: for_line_starts(full),
            scan: for_line_starts(scan),
        }
    }
}

pub struct WrapProcessor {
    right_margin: usize,
    indent_size: usize,
    continuation_indent: usize,
    indent_oracle: Option<IndentOracle>,
    chain_src_break: std::cell::RefCell<Option<ChainSourceBreaks>>,
    chain_nested_call: std::cell::RefCell<HashMap<usize, bool>>,
}

impl WrapProcessor {
    pub fn new(right_margin: usize, indent_size: usize, continuation_indent: usize) -> Self {
        WrapProcessor {
            right_margin,
            indent_size,
            continuation_indent,
            indent_oracle: None,
            chain_src_break: std::cell::RefCell::new(None),
            chain_nested_call: std::cell::RefCell::new(HashMap::new()),
        }
    }

    pub fn set_indent_oracle(&mut self, oracle: IndentOracle) {
        self.indent_oracle = Some(oracle);
    }

    pub fn process(
        &self,
        leaves: &mut [LeafEntry],
        wraps: &HashMap<WrapId, Wrap>,
        positional: &HashSet<WrapId>,
        extra_parents: &HashMap<WrapId, HashSet<WrapId>>,
    ) {
        if self.right_margin == 0 || leaves.is_empty() {
            return;
        }
        scan::WrapScan::new(self, leaves, wraps, positional, extra_parents).run(leaves);
    }
}

fn is_binary_op_text(t: &str) -> bool {
    is_binary_operator_text(t.trim())
}
