use super::{EmitConfig, LeafEntry, current_column, trailing_comment_source_run};
use crate::formatter::ir::AlignmentId;
use std::collections::HashMap;

pub(super) struct LeafMeasurement {
    pub(super) line_indent: usize,
    pub(super) indent_part: usize,
    pub(super) alignment_spaces: usize,
    pub(super) structural_indent: usize,
    pub(super) start_column: usize,
    pub(super) started_line: bool,
}

pub(super) struct LeafHistory {
    pub(super) line_indent: Vec<usize>,
    pub(super) indent_part: Vec<usize>,
    pub(super) alignment_spaces: Vec<usize>,
    pub(super) structural_indent: Vec<usize>,
    pub(super) start_column: Vec<usize>,
    started_line: Vec<bool>,
}

impl LeafHistory {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            line_indent: Vec::with_capacity(capacity),
            indent_part: Vec::with_capacity(capacity),
            alignment_spaces: Vec::with_capacity(capacity),
            structural_indent: Vec::with_capacity(capacity),
            start_column: Vec::with_capacity(capacity),
            started_line: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn record(&mut self, leaf: LeafMeasurement) {
        self.line_indent.push(leaf.line_indent);
        self.indent_part.push(leaf.indent_part);
        self.alignment_spaces.push(leaf.alignment_spaces);
        self.structural_indent.push(leaf.structural_indent);
        self.start_column.push(leaf.start_column);
        self.started_line.push(leaf.started_line);
    }

    pub(super) fn into_results(self) -> (Vec<usize>, Vec<bool>) {
        (self.line_indent, self.started_line)
    }
}

#[derive(Clone, Copy)]
pub(super) enum AlignmentPlacement {
    Inline,
    Wrapped { pure_tabs: bool },
}

#[derive(Clone, Copy)]
pub(super) struct RegisteredAlignment {
    pub(super) column: usize,
    pub(super) indent_part: usize,
    pub(super) placement: AlignmentPlacement,
    leaf: usize,
}

#[derive(Default)]
pub(super) struct AlignmentHistory {
    anchors: HashMap<AlignmentId, RegisteredAlignment>,
}

impl AlignmentHistory {
    pub(super) fn register_first(&mut self, id: AlignmentId) {
        self.anchors.entry(id).or_insert(RegisteredAlignment {
            column: 0,
            indent_part: 0,
            placement: AlignmentPlacement::Inline,
            leaf: 0,
        });
    }

    pub(super) fn register(&mut self, i: usize, leaf: &LeafEntry, out: &str, config: &EmitConfig) {
        for id in leaf
            .alignment
            .into_iter()
            .chain(leaf.align_chain.iter().copied())
        {
            self.anchors.entry(id).or_insert_with(|| {
                let column = current_column(out, config.indent_size)
                    + (leaf.text.len() - leaf.text.trim_start_matches(' ').len());
                let line_indent = out.rsplit('\n').next().unwrap_or("");
                let indent_part = line_indent.bytes().take_while(|b| *b == b'\t').count()
                    * config.indent_size.max(1);
                let placement = if leaf.white_space.line_feeds == 0 {
                    AlignmentPlacement::Inline
                } else {
                    AlignmentPlacement::Wrapped {
                        pure_tabs: !line_indent.is_empty()
                            && line_indent.bytes().all(|b| b == b'\t'),
                    }
                };
                RegisteredAlignment {
                    column,
                    indent_part,
                    placement,
                    leaf: i,
                }
            });
        }
    }

    pub(super) fn inline(&self, id: AlignmentId) -> Option<(usize, usize)> {
        let anchor = self.anchors.get(&id)?;
        match anchor.placement {
            AlignmentPlacement::Inline => Some((anchor.column, anchor.indent_part)),
            AlignmentPlacement::Wrapped { .. } => None,
        }
    }

    pub(super) fn leaf(&self, id: AlignmentId) -> Option<usize> {
        self.anchors.get(&id).map(|a| a.leaf)
    }

    pub(super) fn resolve(
        &self,
        leaf: &LeafEntry,
        parents: &HashMap<AlignmentId, AlignmentId>,
    ) -> Option<RegisteredAlignment> {
        leaf.alignment
            .into_iter()
            .chain(leaf.align_chain.iter().copied())
            .find_map(|id| {
                self.anchors
                    .get(&id)
                    .or_else(|| self.anchors.get(parents.get(&id)?))
                    .copied()
            })
    }
}

pub(super) struct EmissionBuffer {
    pub(super) text: String,
    pub(super) last_text_end: usize,
    pub(super) line_indent: usize,
    pub(super) alignment_spaces: usize,
}

impl EmissionBuffer {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            text: String::with_capacity(capacity),
            last_text_end: 0,
            line_indent: 0,
            alignment_spaces: 0,
        }
    }

    pub(super) fn emit_inline_spacing(
        &mut self,
        leaves: &[LeafEntry],
        i: usize,
        src: &[u8],
        config: &EmitConfig,
    ) {
        let leaf = &leaves[i];
        let verbatim = if leaf.text.starts_with("//") || leaf.text.starts_with("/*") {
            trailing_comment_source_run(leaves, i, src, &self.text)
        } else {
            None
        };
        let spaces = if let Some(run) = verbatim {
            let ts = config.indent_size.max(1);
            run.chars().fold(0, |col, ch| {
                col + if ch == '\t' { ts - col % ts } else { 1 }
            })
        } else {
            leaf.white_space.spaces
        };
        self.text.extend(std::iter::repeat_n(' ', spaces));
    }
}

pub(super) struct DebugFlags {
    pub(super) dot: bool,
    pub(super) binary: bool,
    pub(super) assignment: bool,
    pub(super) alignment: bool,
    pub(super) anchor: bool,
    pub(super) closer: bool,
}

impl DebugFlags {
    pub(super) fn from_env() -> Self {
        Self {
            dot: std::env::var("IJFMT_DOT_DEBUG").is_ok(),
            binary: std::env::var("IJFMT_BINB_DBG").is_ok(),
            assignment: std::env::var("IJFMT_AS_DBG").is_ok(),
            alignment: std::env::var("IJFMT_ALN_DBG").is_ok(),
            anchor: std::env::var("IJFMT_ANCHOR_DEBUG").is_ok(),
            closer: std::env::var("IJFMT_CLOSER_DBG").is_ok(),
        }
    }
}
