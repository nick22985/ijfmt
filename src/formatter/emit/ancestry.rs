use super::super::flatten::{Anchor, ArgCtx, LeafEntry};
use super::super::ir::is_binary_operator_text;
use super::is_binary_op_text;
use std::collections::{HashMap, HashSet};

pub(super) fn arg_enforced(
    ctx: &ArgCtx,
    at: usize,
    elem_firsts: &HashMap<usize, Vec<usize>>,
    own_break: &HashSet<usize>,
    starts: &[bool],
) -> bool {
    if !ctx.smart || !own_break.contains(&ctx.lparen) {
        return false;
    }
    match elem_firsts.get(&ctx.lparen) {
        Some(v) => match v.partition_point(|&e| e <= at).checked_sub(1) {
            Some(k) => !starts.get(v[k]).copied().unwrap_or(false),
            None => false,
        },
        None => false,
    }
}

pub(super) fn chain_anchor_line(
    at: usize,
    leaves: &[LeafEntry],
    line_start_of: &[usize],
    leaf_line_indent: &[usize],
    align_multiline_ternary: bool,
    skip_operator_lines: bool,
) -> (usize, Vec<usize>) {
    let mut ls = line_start_of.get(at).copied().unwrap_or(0);
    let mut scored: Vec<usize> = Vec::new();
    for _ in 0..64 {
        let covers = leaves.get(ls).is_none_or(|l| {
            let span = if l.text == "." {
                (ls..=ls + 2)
                    .filter_map(|p| leaves.get(p).map(|n| n.span_end))
                    .max()
                    .unwrap_or(l.span_end)
            } else if skip_operator_lines && is_binary_operator_text(l.text.trim()) {
                ls
            } else {
                l.span_end
            };
            span >= at && l.text != "?" && !(align_multiline_ternary && l.text == ":")
        });
        if covers {
            break;
        }
        if let Some(lp) = leaves.get(ls).and_then(|l| l.arg_probe_for)
            && !scored.contains(&lp)
        {
            let mut a = match leaves.get(at).and_then(|l| l.anchor.as_ref()) {
                Some(Anchor::Arg(c)) => Some(c),
                _ => None,
            };
            while let Some(c) = a {
                if c.lparen == lp {
                    if c.smart {
                        scored.push(lp);
                    }
                    break;
                }
                a = c.parent.as_ref();
            }
        }
        match ls
            .checked_sub(1)
            .and_then(|p| line_start_of.get(p).copied())
        {
            Some(prev) if prev < ls && leaf_line_indent.get(prev) <= leaf_line_indent.get(ls) => {
                ls = prev;
            }
            _ => break,
        }
    }
    scored.retain(|&lp| lp >= ls);
    if std::env::var("IJFMT_CAL_DBG").is_ok() {
        eprintln!(
            "CAL at={} txt={:?} -> ls={} lstxt={:?} indent={:?} scored={:?}",
            at,
            leaves.get(at).map(|l| l.text.clone()),
            ls,
            leaves.get(ls).map(|l| l.text.clone()),
            leaf_line_indent.get(ls),
            scored
        );
    }
    (ls, scored)
}

pub(super) fn ancestor_line_start(
    at: usize,
    leaves: &[LeafEntry],
    line_start_of: &[usize],
    leaf_line_indent: &[usize],
    align_multiline_ternary: bool,
) -> (usize, bool) {
    ancestor_line_start_opt(
        at,
        leaves,
        line_start_of,
        leaf_line_indent,
        align_multiline_ternary,
        false,
    )
}

pub(super) fn ancestor_line_start_opt(
    at: usize,
    leaves: &[LeafEntry],
    line_start_of: &[usize],
    leaf_line_indent: &[usize],
    align_multiline_ternary: bool,
    skip_operator_lines: bool,
) -> (usize, bool) {
    let mut ls = line_start_of.get(at).copied().unwrap_or(0);
    let mut skipped = false;
    let mut fallback: Option<(usize, bool)> = None;
    let mut guarded = true;
    for _ in 0..64 {
        let glued_kw = leaves.get(ls).is_some_and(|l| l.text == "}")
            && leaves
                .get(ls + 1)
                .is_some_and(|l| matches!(l.text.as_str(), "else" | "catch" | "finally" | "while"));
        let covers = glued_kw
            || leaves.get(ls).is_none_or(|l| {
                l.span_end >= at
                    && l.text != "?"
                    && !(align_multiline_ternary && l.text == ":")
                    && !(skip_operator_lines && l.bin_first.is_some() && is_binary_op_text(&l.text))
            });
        if covers {
            return (ls, skipped);
        }
        let prev = match ls
            .checked_sub(1)
            .and_then(|p| line_start_of.get(p).copied())
        {
            Some(prev) if prev < ls => prev,
            _ => break,
        };
        if guarded && leaf_line_indent.get(prev) > leaf_line_indent.get(ls) {
            guarded = false;
            fallback = Some((ls, skipped));
        }
        ls = prev;
        skipped |= guarded;
    }
    fallback.unwrap_or((ls, skipped))
}
