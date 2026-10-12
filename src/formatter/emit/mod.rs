mod ancestry;
use ancestry::{ancestor_line_start, ancestor_line_start_opt, arg_enforced, chain_anchor_line};
mod anchors;
mod argument_walk;
mod arguments;
mod chains;
mod comments;
mod continuation;
mod facts;
mod indentation;
mod leaf_text;
mod line_indent;
mod state;
mod ternary;

use comments::{
    comment_binds_to_nothing, comment_body_hangs, comment_keeps_first_column,
    is_bare_block_comment, line_comment_trailing_ws, looks_like_commented_code,
    trailing_comment_source_run,
};
use facts::{ChainFacts, LineFacts};
use indentation::{
    compute_alignment_string, compute_indent_string, fill_indent, line_indent_part, source_column,
    source_leading_whitespace,
};
use leaf_text::{emit_leaf_text, emit_raw_source, text_block_enforced};

use state::{AlignmentHistory, EmissionBuffer, LeafHistory, LeafMeasurement};
use std::collections::{HashMap, HashSet};

use super::columns::{current_column, visual_width};
use super::flatten::{Anchor, LeafEntry};
use super::ir::*;

pub struct EmitConfig {
    pub indent_size: usize,
    pub continuation_indent: usize,
    pub use_tabs: bool,
    pub keep_first_column_comment: bool,
    pub align_multiline_ternary: bool,
}

struct LineContext<'a> {
    i: usize,
    leaves: &'a [LeafEntry],
    config: &'a EmitConfig,
    facts: &'a LineFacts,
    history: &'a LeafHistory,
    alignments: &'a AlignmentHistory,
    chain_roots: &'a HashSet<usize>,
    chain_expansion_points: &'a HashMap<usize, Vec<usize>>,
    chain_inner_breaks: &'a HashMap<usize, Vec<usize>>,
    expanded_chains: &'a HashSet<usize>,
    debug_dot: bool,
    debug_binary: bool,
    debug_anchor: bool,
    debug_closer: bool,
}

#[cfg(test)]
pub fn emit(
    leaves: &[LeafEntry],
    config: &EmitConfig,
    src: &[u8],
    alignment_parents: &HashMap<AlignmentId, AlignmentId>,
) -> String {
    emit_with_indents(leaves, config, src, alignment_parents).0
}

pub fn emit_with_indents(
    leaves: &[LeafEntry],
    config: &EmitConfig,
    src: &[u8],
    alignment_parents: &HashMap<AlignmentId, AlignmentId>,
) -> (String, Vec<usize>, Vec<bool>) {
    emit_with_indents_mode(leaves, config, src, alignment_parents, false)
}

pub fn emit_with_indents_mode(
    leaves: &[LeafEntry],
    config: &EmitConfig,
    src: &[u8],
    alignment_parents: &HashMap<AlignmentId, AlignmentId>,
    backward_only: bool,
) -> (String, Vec<usize>, Vec<bool>) {
    Emitter::new(leaves, config, src, alignment_parents, backward_only).emit()
}

struct Emitter<'a> {
    leaves: &'a [LeafEntry],
    config: &'a EmitConfig,
    src: &'a [u8],
    alignment_parents: &'a HashMap<AlignmentId, AlignmentId>,
    backward_only: bool,
    facts: LineFacts,
    chains: ChainFacts,
    chain_roots: HashSet<usize>,
    history: LeafHistory,
    alignments: AlignmentHistory,
    body_anchor_base: HashMap<usize, usize>,
    buffer: EmissionBuffer,
    debug: state::DebugFlags,
}

impl<'a> Emitter<'a> {
    fn new(
        leaves: &'a [LeafEntry],
        config: &'a EmitConfig,
        src: &'a [u8],
        alignment_parents: &'a HashMap<AlignmentId, AlignmentId>,
        backward_only: bool,
    ) -> Self {
        let mut facts = LineFacts::new(leaves.len());
        if !backward_only {
            for i in 0..leaves.len() {
                facts.absorb(i, leaves, config, src);
            }
        }
        let chains = ChainFacts::new(leaves, &facts.chain_dot_breaks);
        Self {
            leaves,
            config,
            src,
            alignment_parents,
            backward_only,
            facts,
            chains,
            chain_roots: leaves.iter().filter_map(|l| l.chain_probe_for).collect(),
            history: LeafHistory::new(leaves.len()),
            alignments: AlignmentHistory::default(),
            body_anchor_base: HashMap::new(),
            buffer: EmissionBuffer::new(src.len()),
            debug: state::DebugFlags::from_env(),
        }
    }

    fn emit(mut self) -> (String, Vec<usize>, Vec<bool>) {
        for i in 0..self.leaves.len() {
            self.emit_leaf(i);
        }
        let (indents, starts) = self.history.into_results();
        (self.buffer.text, indents, starts)
    }

    fn emit_leaf(&mut self, i: usize) {
        let leaf = &self.leaves[i];
        if self.backward_only {
            self.facts.absorb(i, self.leaves, self.config, self.src);
        }
        let mut indent = compute_indent_string(&leaf.indent_stack, self.config);
        let structural_indent = visual_width(&indent, self.config.indent_size);
        if i == 0
            && let Some(id) = leaf.alignment
        {
            self.alignments.register_first(id);
        }
        if i > 0 && !leaf.raw_source {
            if leaf.white_space.line_feeds > 0 {
                comments::emit_leading_lines(
                    &mut self.buffer.text,
                    &mut self.buffer.last_text_end,
                    leaf,
                    self.src,
                    self.config,
                );
                let context = LineContext {
                    i,
                    leaves: self.leaves,
                    config: self.config,
                    facts: &self.facts,
                    history: &self.history,
                    alignments: &self.alignments,
                    chain_roots: &self.chain_roots,
                    chain_expansion_points: &self.chains.expansion_points,
                    chain_inner_breaks: &self.chains.inner_breaks,
                    expanded_chains: &self.chains.expanded,
                    debug_dot: self.debug.dot,
                    debug_binary: self.debug.binary,
                    debug_anchor: self.debug.anchor,
                    debug_closer: self.debug.closer,
                };
                context.emit_line_indent(
                    &mut self.buffer,
                    &mut indent,
                    line_indent::IndentEnvironment {
                        src: self.src,
                        alignment_parents: self.alignment_parents,
                        body_anchor_base: &mut self.body_anchor_base,
                        debug: &self.debug,
                    },
                );
            } else {
                self.buffer
                    .emit_inline_spacing(self.leaves, i, self.src, self.config);
            }
            self.alignments
                .register(i, leaf, &self.buffer.text, self.config);
            if leaf.white_space.line_feeds > 0
                && let Some(trail) = line_comment_trailing_ws(
                    self.leaves,
                    i,
                    self.src,
                    &self.buffer.text[..self.buffer.last_text_end],
                )
            {
                self.buffer
                    .text
                    .insert_str(self.buffer.last_text_end, trail);
            }
        }
        let measurement = LeafMeasurement {
            line_indent: self.buffer.line_indent,
            indent_part: line_indent_part(&self.buffer.text, self.config.indent_size),
            alignment_spaces: self.buffer.alignment_spaces,
            structural_indent,
            start_column: current_column(&self.buffer.text, self.config.indent_size),
            started_line: i == 0 || leaf.white_space.line_feeds > 0,
        };
        if leaf.raw_source {
            emit_raw_source(&mut self.buffer.text, &leaf.text);
        } else {
            emit_leaf_text(
                &mut self.buffer.text,
                leaf,
                &indent,
                self.config,
                self.src,
                self.buffer.line_indent,
                text_block_enforced(i, self.leaves),
            );
        }
        self.buffer.last_text_end = self.buffer.text.len();
        self.history.record(measurement);
    }
}

fn spans_match(out_span: &[u8], src_span: &[u8]) -> bool {
    let mut it = out_span.iter();
    for &c in src_span {
        if c == b'\r' {
            continue;
        }
        if it.next() != Some(&c) {
            return false;
        }
    }
    it.next().is_none()
}

fn change_touches_last_line(old: &[u8], new: &[u8]) -> bool {
    if old == new {
        return false;
    }
    let mut p = 0;
    while p < old.len() && p < new.len() && old[p] == new[p] {
        p += 1;
    }
    let mut s = 0;
    while s < old.len() - p && s < new.len() - p && old[old.len() - 1 - s] == new[new.len() - 1 - s]
    {
        s += 1;
    }
    let change_end = old.len() - s;
    let last_line_start = old
        .iter()
        .rposition(|&c| c == b'\n')
        .map(|q| q + 1)
        .unwrap_or(0);
    if p == change_end {
        p >= last_line_start
    } else {
        change_end > last_line_start
    }
}

fn strip_trailing_ws(out: &mut String) {
    while out.ends_with(' ') || out.ends_with('\t') {
        out.pop();
    }
}

fn strip_trailing_spaces(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
}

fn strip_trailing_ws_guarded(out: &mut String, guard: usize) {
    while out.len() > guard && (out.ends_with(' ') || out.ends_with('\t')) {
        out.pop();
    }
}

fn is_binary_op_text(t: &str) -> bool {
    is_binary_operator_text(t.trim())
}

#[cfg(test)]
mod tests {
    use super::{is_binary_op_text, is_binary_operator_text};

    #[test]
    fn operator_helpers_keep_their_distinct_whitespace_rules() {
        for operator in [
            "+",
            "-",
            "*",
            "/",
            "%",
            "&&",
            "||",
            "&",
            "|",
            "^",
            "<<",
            ">>",
            ">>>",
            "==",
            "!=",
            "<",
            ">",
            "<=",
            ">=",
            "instanceof",
        ] {
            assert!(is_binary_operator_text(operator));
            assert!(is_binary_op_text(operator));
            let spaced = format!(" {operator}\t");
            assert!(!is_binary_operator_text(&spaced));
            assert!(is_binary_op_text(&spaced));
        }
        assert!(!is_binary_op_text("??"));
    }
}
