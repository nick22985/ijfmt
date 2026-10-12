mod leaf;
mod traversal;

use super::ir::{Block, IndentType};
pub use leaf::{Anchor, ArgCtx, LeafEntry, WhiteSpace};
use traversal::Traversal;

pub fn flatten(root: &Block, entries: &mut Vec<LeafEntry>) {
    entries.clear();
    Traversal::default().visit(root, entries, None);

    for i in 1..entries.len() {
        if !entries[i - 1].line_comment {
            continue;
        }
        let mut j = i;
        while j < entries.len()
            && entries[j].text.is_empty()
            && entries[j].white_space.line_feeds == 0
        {
            j += 1;
        }
        if j < entries.len() && entries[j].white_space.line_feeds == 0 {
            entries[j].white_space.line_feeds = 1;
            entries[j].white_space.spaces = 0;
        }
    }

    if std::env::var("IJFMT_STACK_DBG").is_ok() {
        for (i, e) in entries.iter().enumerate() {
            eprintln!(
                "[STK] {i} {:?} lf={} {:?}",
                e.text, e.white_space.line_feeds, e.indent_stack
            );
        }
    }
}

pub(crate) fn resolve_conditional_indents(leaves: &mut [LeafEntry]) {
    let starts: Vec<bool> = leaves
        .iter()
        .enumerate()
        .map(|(i, l)| i == 0 || l.white_space.line_feeds > 0)
        .collect();
    for leaf in leaves.iter_mut() {
        for entry in leaf.indent_stack.iter_mut() {
            if let IndentType::NormalIfWrapped(at) = *entry {
                *entry = if starts.get(at).copied().unwrap_or(true) {
                    IndentType::Normal
                } else {
                    IndentType::None
                };
            } else if let IndentType::ContinuationIfWrapped(at) = *entry {
                *entry = if starts.get(at).copied().unwrap_or(true) {
                    IndentType::Continuation
                } else {
                    IndentType::None
                };
            }
        }
    }
}
