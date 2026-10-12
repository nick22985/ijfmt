use super::super::ir::*;
use super::{Anchor, ArgCtx, LeafEntry, WhiteSpace};

struct BinFrame {
    paren_lparen: Option<usize>,
    first: usize,
    encl_lparen: Option<usize>,
}

struct ChainFrame {
    id: usize,
    arg_count: usize,
    cur_seg_first: Option<usize>,
    encl_lparen: Option<usize>,
}

#[derive(Default)]
pub(super) struct Traversal {
    indent_stack: Vec<IndentType>,
    pending_wraps: Vec<WrapId>,
    pending_alignment: Option<AlignmentId>,
    ancestor_first_leaves: Vec<usize>,
    anchor_ctx: Vec<Anchor>,
    pending_probe: Option<usize>,
    chain_stack: Vec<ChainFrame>,
    pending_chain_probe: Option<usize>,
    pending_chain_dot: Option<(usize, u8, Option<usize>)>,
    bin_stack: Vec<BinFrame>,
    pending_bin: Option<(usize, Option<usize>, Option<usize>)>,
    bin_scope_stack: Vec<usize>,
    pending_tern_colon: Option<(usize, Option<usize>)>,
    tern_scope_stack: Vec<usize>,
    tern_cmt_stack: Vec<usize>,
    tern_self_stack: Vec<usize>,
    pending_tern_self: Option<usize>,
    pending_tern_branch: Option<usize>,
    pending_tern_cmt: Option<usize>,
    pending_tern_nest: Option<(usize, bool)>,
    pending_tern_nest_col: Option<usize>,
    pending_tern_signcol: bool,
    pending_tern_bin: Option<usize>,
    pending_bin_first: Option<usize>,
    arr_scope_stack: Vec<usize>,
    arr_elem_stack: Vec<usize>,
    pending_arr: Option<usize>,
    pending_arr_close: bool,
    param_scope_stack: Vec<usize>,
    pending_param: Option<usize>,
    break_bonus_stack: Vec<(usize, Option<usize>)>,
    lam_cont_stack: Vec<usize>,
    pending_paren_cwf: Option<usize>,
    pending_res_align: bool,
    align_scope_stack: Vec<(AlignmentId, usize, usize)>,
    first_leaf_stack: Vec<usize>,
}

struct ScopeFrame {
    pushed_anchor_body: bool,
    pushed_chain: bool,
    pushed_bin: bool,
    pushed_arr_elem: bool,
    pushed_break_bonus: bool,
    pushed_tern_self_scope: bool,
    pushed_tern_cmt_scope: bool,
    pushed_tern_scope: bool,
    pushed_arg_anchor: bool,
    saved_continuations: Vec<(usize, IndentType)>,
    saved_seg_first: Option<Option<usize>>,
}

impl Traversal {
    pub(super) fn visit(
        &mut self,
        block: &Block,
        entries: &mut Vec<LeafEntry>,
        spacing: Option<&Spacing>,
    ) {
        let span_start = entries.len();
        let scope = self.enter_scope(block, entries.len());

        if let Some(ref wrap) = block.wrap {
            self.pending_wraps.push(wrap.id);
        }

        if block.alignment.is_some() {
            self.pending_alignment = block.alignment;
        }
        if block.align_barrier {
            self.first_leaf_stack.push(usize::MAX);
        }
        self.first_leaf_stack.push(entries.len());
        let my_depth = self.first_leaf_stack.len() - 1;
        let pushed_align_scope = match block.alignment {
            Some(aid) if !block.is_leaf() => {
                self.align_scope_stack.push((aid, entries.len(), my_depth));
                true
            }
            _ => false,
        };

        if block.is_leaf() {
            self.append_leaf(block, entries, spacing);
        } else {
            let mut first = true;
            for child in &block.children {
                let child_spacing = if first {
                    first = false;
                    block.spacing.as_ref().or(spacing)
                } else {
                    child.spacing.as_ref()
                };

                self.visit(child, entries, child_spacing);
            }
        }

        if entries.len() > span_start {
            let last = entries.len() - 1;
            if entries[span_start].span_end < last {
                entries[span_start].span_end = last;
            }
        }

        self.leave_scope(block, scope, pushed_align_scope);
    }

    fn append_leaf(
        &mut self,
        block: &Block,
        entries: &mut Vec<LeafEntry>,
        spacing: Option<&Spacing>,
    ) {
        let leaf_idx = entries.len();
        let effective_spacing = block.spacing.as_ref().or(spacing);

        let mut ws = WhiteSpace::new();
        if let Some(sp) = effective_spacing {
            debug_assert!(sp.min_spaces <= sp.max_spaces);
            ws.line_feeds = sp.min_line_feeds as usize;
            if sp.keep_line_breaks && block.source_has_line_break_before && ws.line_feeds == 0 {
                ws.line_feeds = 1;
            }
            if ws.line_feeds == 0 {
                ws.spaces = sp.min_spaces as usize;
            }
        }

        let wraps = std::mem::take(&mut self.pending_wraps);

        let alignment = block.alignment.or(self.pending_alignment.take());

        entries.push(LeafEntry {
            text: block.text.clone(),
            wraps,
            white_space: ws,
            indent_stack: self.indent_stack.clone(),
            alignment,
            source_offset: block.source_offset,
            source_has_line_break_before: block.source_has_line_break_before,
            raw_source: block.raw_source,
            jd_regenerated: block.jd_regenerated,
            extra_width: block.extra_width,
            pre_run_width: block.pre_run_width,
            line_comment: block.line_comment,
            statement_comment: block.statement_comment,
            member_bound_comment: block.member_bound_comment,
            anchor: self.anchor_ctx.last().cloned(),
            anchor_self_exempt: block.anchor_exempt,
            paren_close: block.paren_close_probe,
            arg_probe_for: self.pending_probe.take(),
            chain_probe_for: self.pending_chain_probe.take(),
            chain_dot: self.pending_chain_dot.take(),
            bin_op: self.pending_bin.take(),
            tern_colon: self.pending_tern_colon.take(),
            tern_nest: self.pending_tern_nest.take(),
            tern_nest_col: self.pending_tern_nest_col.take(),
            tern_signcol: std::mem::take(&mut self.pending_tern_signcol),
            tern_cmt: self.pending_tern_cmt.take(),
            tern_self: self.pending_tern_self.take(),
            tern_branch: self.pending_tern_branch.take(),
            tern_bin: self.pending_tern_bin.take(),
            bin_first: self.pending_bin_first.take(),
            arr_first: self.pending_arr.take(),
            arr_close: std::mem::take(&mut self.pending_arr_close),
            arr_elem: self.arr_elem_stack.last().copied(),
            param_first: self.pending_param.take(),
            span_end: entries.len(),
            break_bonus: self.break_bonus_stack.last().copied(),
            lambda_body_cont: self.lam_cont_stack.last().copied(),
            paren_cwf: self.pending_paren_cwf.take(),
            res_align: std::mem::take(&mut self.pending_res_align),
            align_scope: self
                .align_scope_stack
                .iter()
                .rev()
                .find(|(aid, bf, d)| {
                    if Some(*aid) == alignment {
                        return false;
                    }
                    let mut j = d + 1;
                    while self.first_leaf_stack.get(j) == Some(bf) {
                        j += 1;
                    }
                    self.first_leaf_stack.get(j).copied().unwrap_or(leaf_idx) == leaf_idx
                })
                .map(|(aid, bf, _)| (*aid, *bf)),
            align_chain: self
                .align_scope_stack
                .iter()
                .rev()
                .filter(|(aid, bf, _)| *bf == leaf_idx && Some(*aid) != alignment)
                .map(|(aid, _, _)| *aid)
                .collect(),
            encl_chain: {
                let mut v: Vec<usize> = Vec::new();
                for &f in self.first_leaf_stack.iter().rev() {
                    if f == usize::MAX || f >= leaf_idx {
                        continue;
                    }
                    if v.last() == Some(&f) {
                        continue;
                    }
                    v.push(f);
                }
                v
            },
        });
    }

    fn enter_indent(&mut self, indent: IndentType, leaf_index: usize) -> Vec<(usize, IndentType)> {
        let saved_continuations: Vec<(usize, IndentType)>;
        if indent == IndentType::BlockReset {
            saved_continuations = self
                .indent_stack
                .iter()
                .enumerate()
                .filter(|(_, i)| matches!(i, IndentType::Continuation))
                .map(|(k, i)| (k, *i))
                .collect();
            self.indent_stack
                .retain(|i| !matches!(i, IndentType::Continuation));
        } else if indent == IndentType::CancelOneContinuation {
            let pos = self
                .indent_stack
                .iter()
                .rposition(|i| matches!(i, IndentType::Continuation));
            saved_continuations = if let Some(p) = pos {
                vec![(p, self.indent_stack.remove(p))]
            } else {
                Vec::new()
            };
        } else if indent == IndentType::CancelOneNormal {
            let pos = self
                .indent_stack
                .iter()
                .rposition(|i| matches!(i, IndentType::Normal));
            saved_continuations = if let Some(p) = pos {
                vec![(p, self.indent_stack.remove(p))]
            } else {
                Vec::new()
            };
        } else if indent == IndentType::BlockResetKeepOne {
            let cont_count = self
                .indent_stack
                .iter()
                .filter(|i| matches!(i, IndentType::Continuation))
                .count();
            if cont_count > 1 {
                let mut kept = false;
                let mut stripped = Vec::new();
                let mut new_stack = Vec::new();
                for (k, entry) in self.indent_stack.iter().enumerate().rev() {
                    if matches!(entry, IndentType::Continuation) && !kept {
                        kept = true;
                        new_stack.push(*entry);
                    } else if matches!(entry, IndentType::Continuation) {
                        stripped.push((k, *entry));
                    } else {
                        new_stack.push(*entry);
                    }
                }
                stripped.reverse();
                new_stack.reverse();
                self.indent_stack = new_stack;
                saved_continuations = stripped;
            } else {
                saved_continuations = Vec::new();
            }
        } else if indent == IndentType::ColumnZero {
            saved_continuations = self.indent_stack.drain(..).enumerate().collect();
        } else {
            saved_continuations = Vec::new();
            if indent != IndentType::None {
                self.indent_stack.push(match indent {
                    IndentType::NormalIfWrapped(usize::MAX) => {
                        IndentType::NormalIfWrapped(leaf_index)
                    }
                    IndentType::ContinuationIfWrapped(usize::MAX) => {
                        IndentType::ContinuationIfWrapped(leaf_index)
                    }
                    other => other,
                });
            }
        }

        saved_continuations
    }

    fn enter_scope(&mut self, block: &Block, leaf_index: usize) -> ScopeFrame {
        if block.anchor_start {
            self.ancestor_first_leaves.push(leaf_index);
        }
        let pushed_anchor_body = if block.anchor_body {
            if let Some(&a) = self.ancestor_first_leaves.last() {
                let encl_arg = self.anchor_ctx.iter().rev().find_map(|x| match x {
                    Anchor::Arg(ctx) => Some(ctx.clone()),
                    _ => None,
                });
                self.anchor_ctx.push(Anchor::Body(
                    std::rc::Rc::new(vec![a]),
                    usize::MAX,
                    encl_arg,
                    leaf_index,
                ));
                true
            } else {
                false
            }
        } else {
            false
        };
        if block.arg_probe {
            let lp = self.anchor_ctx.iter().rev().find_map(|a| match a {
                Anchor::Arg(ctx) => Some(ctx.lparen),
                _ => None,
            });
            if lp.is_some() {
                self.pending_probe = lp;
            }
        }
        let pushed_chain = if block.chain_anchor {
            let arg_count = self
                .anchor_ctx
                .iter()
                .filter(|a| matches!(a, Anchor::Arg(_)))
                .count();
            let encl_lparen = self.anchor_ctx.iter().rev().find_map(|a| match a {
                Anchor::Arg(ctx) => Some(ctx.lparen),
                _ => None,
            });
            self.chain_stack.push(ChainFrame {
                id: leaf_index,
                arg_count,
                cur_seg_first: None,
                encl_lparen,
            });
            true
        } else {
            false
        };
        let pushed_bin = if block.bin_anchor {
            let encl_lparen = self
                .anchor_ctx
                .iter()
                .rev()
                .map(|a| match a {
                    Anchor::Arg(ctx) => Some(ctx.lparen),
                    Anchor::Body(..) => None,
                })
                .next()
                .flatten();
            self.bin_stack.push(BinFrame {
                first: leaf_index,
                encl_lparen,
                paren_lparen: None,
            });
            true
        } else if block.bin_paren_anchor && leaf_index > 0 && self.bin_stack.is_empty() {
            self.bin_stack.push(BinFrame {
                first: leaf_index,
                encl_lparen: None,
                paren_lparen: Some(leaf_index - 1),
            });
            true
        } else {
            false
        };
        if block.tern_bin_probe
            && let Some(&first) = self.bin_scope_stack.last()
        {
            self.pending_tern_bin = Some(first);
        }
        if block.bin_child_probe
            && let Some(&first) = self.bin_scope_stack.last()
        {
            self.pending_bin_first = Some(first);
        }
        if block.arr_probe
            && let Some(&first) = self.arr_scope_stack.last()
        {
            self.pending_arr = Some(first);
            self.pending_arr_close = block.arr_close;
        }
        if block.param_probe
            && let Some(&first) = self.param_scope_stack.last()
        {
            self.pending_param = Some(first);
        }
        if block.arr_scope {
            self.arr_scope_stack.push(leaf_index);
        }
        let pushed_arr_elem = if block.arr_probe && !block.arr_close {
            self.arr_elem_stack.push(leaf_index);
            true
        } else {
            false
        };
        if block.param_scope {
            self.param_scope_stack.push(leaf_index);
        }
        if block.bin_scope {
            self.bin_scope_stack.push(leaf_index);
        }
        if block.paren_cwf_probe && leaf_index > 0 {
            self.pending_paren_cwf = Some(leaf_index - 1);
        }
        if block.res_align {
            self.pending_res_align = true;
        }
        let pushed_break_bonus = if block.break_bonus {
            let gate = if block.break_bonus_gate {
                self.first_leaf_stack
                    .iter()
                    .rev()
                    .find(|&&f| f != usize::MAX)
                    .copied()
            } else {
                None
            };
            self.break_bonus_stack.push((leaf_index, gate));
            true
        } else {
            false
        };
        if block.lambda_body_cont {
            self.lam_cont_stack.push(leaf_index);
        }
        let pushed_tern_self_scope = if block.tern_self_scope {
            self.tern_self_stack.push(leaf_index);
            true
        } else {
            false
        };
        if block.tern_self_probe
            && let Some(&first) = self.tern_self_stack.last()
        {
            self.pending_tern_self = Some(first);
        }
        if block.tern_branch_probe
            && let Some(&first) = self.tern_self_stack.last()
        {
            self.pending_tern_branch = Some(first);
        }
        let pushed_tern_cmt_scope = if block.tern_cmt_scope {
            self.tern_cmt_stack.push(leaf_index);
            true
        } else {
            false
        };
        if block.tern_cmt_probe
            && let Some(&first) = self.tern_cmt_stack.last()
        {
            self.pending_tern_cmt = Some(first);
        }
        let pushed_tern_scope = if block.tern_scope {
            self.tern_scope_stack.push(leaf_index);
            true
        } else {
            false
        };
        if block.tern_nest_probe
            && let Some(&first) = self.tern_scope_stack.last()
        {
            self.pending_tern_nest = Some((first, block.tern_nest_deep_probe));
        }
        if block.tern_nest_col_probe
            && let Some(&first) = self.tern_scope_stack.last()
        {
            self.pending_tern_nest_col = Some(first);
        }
        if block.tern_signcol_probe {
            self.pending_tern_signcol = true;
        }
        if block.tern_colon_probe {
            let own = self.tern_self_stack.last().copied();
            if let Some(&first) = self.bin_scope_stack.last() {
                self.pending_tern_colon = Some((first, own));
            } else if let Some(first) = own {
                self.pending_tern_colon = Some((first, own));
            }
        }
        if block.bin_probe
            && let Some(frame) = self.bin_stack.last()
        {
            self.pending_bin = Some((frame.first, frame.encl_lparen, frame.paren_lparen));
        }
        let saved_seg_first = if block.chain_probe {
            if let Some(frame) = self.chain_stack.last_mut() {
                self.pending_chain_probe = Some(frame.id);
                if block.chain_seg_channel != 0 {
                    self.pending_chain_dot =
                        Some((frame.id, block.chain_seg_channel, frame.encl_lparen));
                }
                let saved = frame.cur_seg_first;
                frame.cur_seg_first = Some(leaf_index);
                Some(saved)
            } else {
                None
            }
        } else {
            None
        };
        let saved_continuations = self.enter_indent(block.indent, leaf_index);

        if pushed_anchor_body
            && let Some(Anchor::Body(_, split, _, _)) = self.anchor_ctx.last_mut()
            && *split == usize::MAX
        {
            *split = self.indent_stack.len();
        }

        let pushed_arg_anchor = if let Some(info) = block.arg_list_anchor {
            let parent = self.anchor_ctx.iter().rev().find_map(|a| match a {
                Anchor::Arg(ctx) => Some(ctx.clone()),
                _ => None,
            });
            let body = self.anchor_ctx.iter().rev().find_map(|a| match a {
                Anchor::Body(cands, split, _, brace) => Some((cands.clone(), *split, *brace)),
                _ => None,
            });
            let arg_count = self
                .anchor_ctx
                .iter()
                .filter(|a| matches!(a, Anchor::Arg(_)))
                .count();
            let (chain, seg_first) = match self.chain_stack.last() {
                Some(frame) if frame.arg_count == arg_count => {
                    (Some(frame.id), frame.cur_seg_first)
                }
                _ => (None, None),
            };
            if std::env::var("IJFMT_ANCHOR_DEBUG").is_ok() {
                eprintln!(
                    "PUSH ArgCtx lparen={} channel={} elem_has_cont={} collapsed={} seg_cancel={} bin_supp={}",
                    leaf_index,
                    info.channel,
                    info.elem_has_cont,
                    info.collapsed,
                    info.seg_cancel,
                    info.bin_suppressed
                );
            }
            self.anchor_ctx.push(Anchor::Arg(std::rc::Rc::new(ArgCtx {
                lparen: leaf_index,
                elem_has_cont: info.elem_has_cont,
                channel: info.channel,
                chain_channel: info.chain_channel,
                chain_wrapper_static: info.chain_wrapper_static,
                chain_line_capped: info.chain_line_capped,
                seg_cancel: info.seg_cancel,
                collapsed: info.collapsed,
                enum_const: info.enum_const,
                bin_suppressed: info.bin_suppressed,
                smart: info.smart,
                ignore_parent_wraps: info.ignore_parent_wraps,
                chain,
                seg_first,
                parent,
                body,
            })));
            true
        } else {
            false
        };

        ScopeFrame {
            pushed_anchor_body,
            pushed_chain,
            pushed_bin,
            pushed_arr_elem,
            pushed_break_bonus,
            pushed_tern_self_scope,
            pushed_tern_cmt_scope,
            pushed_tern_scope,
            pushed_arg_anchor,
            saved_continuations,
            saved_seg_first,
        }
    }

    fn leave_scope(&mut self, block: &Block, scope: ScopeFrame, pushed_align_scope: bool) {
        if matches!(
            block.indent,
            IndentType::BlockReset
                | IndentType::CancelOneContinuation
                | IndentType::CancelOneNormal
                | IndentType::BlockResetKeepOne
                | IndentType::ColumnZero
        ) {
            for (k, cont) in scope.saved_continuations {
                let at = k.min(self.indent_stack.len());
                self.indent_stack.insert(at, cont);
            }
        } else if block.indent != IndentType::None {
            self.indent_stack.pop();
        }

        if scope.pushed_arg_anchor {
            self.anchor_ctx.pop();
        }
        if scope.pushed_anchor_body {
            self.anchor_ctx.pop();
        }
        if scope.pushed_tern_cmt_scope {
            self.tern_cmt_stack.pop();
        }
        if scope.pushed_tern_self_scope {
            self.tern_self_stack.pop();
        }
        if scope.pushed_tern_scope {
            self.tern_scope_stack.pop();
        }
        if scope.pushed_break_bonus {
            self.break_bonus_stack.pop();
        }
        if block.lambda_body_cont {
            self.lam_cont_stack.pop();
        }
        if block.arr_scope {
            self.arr_scope_stack.pop();
        }
        if scope.pushed_arr_elem {
            self.arr_elem_stack.pop();
        }
        if block.param_scope {
            self.param_scope_stack.pop();
        }
        if block.bin_scope {
            self.bin_scope_stack.pop();
        }
        if pushed_align_scope {
            self.align_scope_stack.pop();
        }
        self.first_leaf_stack.pop();
        if block.align_barrier {
            self.first_leaf_stack.pop();
        }
        if scope.pushed_bin {
            self.bin_stack.pop();
        }
        if let Some(saved) = scope.saved_seg_first
            && let Some(frame) = self.chain_stack.last_mut()
        {
            frame.cur_seg_first = saved;
        }
        if scope.pushed_chain {
            self.chain_stack.pop();
        }
        if block.anchor_start {
            self.ancestor_first_leaves.pop();
        }
    }
}
