use super::super::columns::visual_width;
use super::super::flatten::Anchor;
use super::super::ir::IndentType;
use super::{LineContext, chain_anchor_line, is_binary_op_text};

pub(super) struct ChainIndent {
    pub(super) columns: Option<usize>,
    pub(super) may_lower: bool,
}

impl LineContext<'_> {
    pub(super) fn chain_indent(&self, indent: &str) -> ChainIndent {
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf_align_spaces = &self.history.alignment_spaces;
        let debug_dot = self.debug_dot;
        let leaf = &self.leaves[self.i];
        let mut return_dot_override: Option<usize> = None;
        if debug_dot && let Some((cr, lvl, _)) = leaf.chain_dot {
            eprintln!(
                "DOT leaf={:?} lvl={} root={:?} root_line_start={:?} root_line_indent={:?} root_line_align={:?} starts_root={} anchor={:?}",
                leaf.text.chars().take(14).collect::<String>(),
                lvl,
                leaves[cr].text,
                facts.line_start_of.get(cr).map(|&l| leaves[l].text.clone()),
                leaf_line_indent.get(cr),
                facts
                    .line_start_of
                    .get(cr)
                    .and_then(|&l| leaf_align_spaces.get(l)),
                facts.starts.get(cr).copied().unwrap_or(false),
                match &leaves[cr].anchor {
                    Some(Anchor::Arg(c)) => format!(
                        "Arg(lp={} ownbrk={} sameline={})",
                        c.lparen,
                        facts.own_break.contains(&c.lparen),
                        facts.line_no.get(cr) == facts.line_no.get(c.lparen)
                    ),
                    other => format!("{:?}", other.is_some()),
                }
            );
        }
        let mut dot_override: Option<usize> = match leaf.chain_dot {
            Some((chain_root, 3, _)) => {
                let (als, scored) = chain_anchor_line(
                    chain_root,
                    leaves,
                    &facts.line_start_of,
                    leaf_line_indent,
                    config.align_multiline_ternary,
                    true,
                );
                leaf_line_indent
                    .get(als)
                    .map(|&b| b + (scored.len() + 1) * config.continuation_indent)
            }
            Some((chain_root, 1, _))
                if leaves[facts
                    .line_start_of
                    .get(chain_root)
                    .copied()
                    .unwrap_or(chain_root)]
                .text
                .as_str()
                    == "?"
                    || (chain_root > 0
                        && leaves[chain_root - 1].text.trim() == "?"
                        && facts.line_no.get(chain_root - 1) == facts.line_no.get(chain_root)
                        && matches!(
                            leaves[facts
                                .line_start_of
                                .get(chain_root)
                                .copied()
                                .unwrap_or(chain_root)]
                            .text
                            .as_str(),
                            ":"
                        )) =>
            {
                let ls = facts
                    .line_start_of
                    .get(chain_root)
                    .copied()
                    .unwrap_or(chain_root);
                let al = leaf_align_spaces.get(ls).copied().unwrap_or(0);
                leaf_line_indent.get(ls).map(|&b| {
                    if al > 0 {
                        b - al.min(b) + config.continuation_indent
                    } else {
                        b
                    }
                })
            }
            Some((chain_root, 1, _))
                if facts.starts.get(chain_root).copied().unwrap_or(false) || {
                    let ls = facts.line_start_of.get(chain_root).copied().unwrap_or(0);
                    let stmt_line = !leaves[ls]
                        .indent_stack
                        .iter()
                        .any(|x| matches!(x, IndentType::Continuation));
                    let lambda_body_root = chain_root > 0
                        && leaves[chain_root - 1].text.trim_end().ends_with("->")
                        && !facts.starts.get(chain_root).copied().unwrap_or(false);
                    let elem_line_scores = match leaves[ls].arg_probe_for {
                        Some(lp)
                            if (ls + 1..=chain_root)
                                .any(|p| leaves[p].arg_probe_for == Some(lp)) =>
                        {
                            let mut a = match &leaves[ls].anchor {
                                Some(Anchor::Arg(c)) => Some(c),
                                _ => None,
                            };
                            let mut smart = true;
                            while let Some(c) = a {
                                if c.lparen == lp {
                                    smart = c.smart;
                                    break;
                                }
                                a = c.parent.as_ref();
                            }
                            smart
                        }
                        _ => true,
                    };
                    let elem_line = matches!(&leaves[ls].anchor, Some(Anchor::Arg(_)))
                        && elem_line_scores
                        && leaves[ls].bin_op.is_none()
                        && !matches!(
                            leaves[ls].text.trim(),
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
                        );
                    let ternary_sign_line = leaves[ls].text.as_str() == ":";
                    let paren_line = leaves[ls].text.as_str() == "(";
                    let root_line_text = leaves[ls].text.trim();
                    let cont_marker_line = leaf_align_spaces.get(ls).copied().unwrap_or(0) == 0
                        && ((root_line_text.len() > 1 && root_line_text.starts_with('.'))
                            || matches!(
                                root_line_text,
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
                            ));
                    let root_in_line_opened_list = leaf_align_spaces.get(ls).copied().unwrap_or(0)
                        == 0
                        && match &leaves[chain_root].anchor {
                            Some(Anchor::Arg(c)) => {
                                c.lparen > ls
                                    && facts.line_no.get(c.lparen) == facts.line_no.get(chain_root)
                            }
                            _ => false,
                        };
                    let after_open_paren =
                        ls > 0 && matches!(leaves[ls - 1].text.trim(), "(" | "?" | ":");
                    let bin_operand_root = after_open_paren
                        && (ls + 1..chain_root).any(|j| {
                            matches!(
                                leaves[j].text.trim(),
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
                                    | "?"
                                    | ":"
                            )
                        });
                    let nested_binary_rhs_root = leaves[ls]
                        .bin_first
                        .zip(leaves[chain_root].bin_first)
                        .is_some_and(|(line_binary, root_binary)| {
                            line_binary != root_binary
                                && (ls + 1..chain_root).any(|j| is_binary_op_text(&leaves[j].text))
                        });
                    let unary_prefix_root = ls + 1 == chain_root
                        && matches!(leaves[ls].text.as_str(), "!" | "~" | "+" | "-");
                    let stmt_kw_line = matches!(
                        leaves[ls].text.as_str(),
                        "if" | "while"
                            | "return"
                            | "for"
                            | "switch"
                            | "throw"
                            | "synchronized"
                            | "do"
                            | "assert"
                            | "yield"
                            | "case"
                    );
                    let aligned_op_line = leaf_align_spaces.get(ls).copied().unwrap_or(0) > 0
                        && leaves[ls].bin_first.is_some()
                        && matches!(
                            root_line_text,
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
                        );
                    cont_marker_line
                        || aligned_op_line
                        || stmt_line
                        || stmt_kw_line
                        || lambda_body_root
                        || elem_line
                        || ternary_sign_line
                        || paren_line
                        || bin_operand_root
                        || nested_binary_rhs_root
                        || unary_prefix_root
                        || root_in_line_opened_list
                } =>
            {
                let cont_marker_root = {
                    let ls = facts.line_start_of.get(chain_root).copied().unwrap_or(0);
                    let t = leaves[ls].text.trim();
                    let op_line = matches!(
                        t,
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
                    );
                    let unary_prefix_line = ls + 1 == chain_root
                        && leaves[ls].bin_first.is_none()
                        && matches!(t, "+" | "-");
                    let cml = leaf_align_spaces.get(ls).copied().unwrap_or(0) == 0
                        && !unary_prefix_line
                        && ((t.len() > 1 && t.starts_with('.')) || op_line);
                    let ril = match &leaves[chain_root].anchor {
                        Some(Anchor::Arg(c)) => {
                            c.lparen > ls
                                && facts.line_no.get(c.lparen) == facts.line_no.get(chain_root)
                        }
                        _ => false,
                    };
                    let bin_line =
                        if op_line && !ril && leaf_align_spaces.get(ls).copied().unwrap_or(0) > 0 {
                            leaves[ls]
                                .bin_first
                                .map(|bf| facts.line_start_of.get(bf).copied().unwrap_or(bf))
                                .and_then(|bl| leaf_line_indent.get(bl).copied())
                                .map(|b| b + config.continuation_indent)
                        } else {
                            None
                        };
                    bin_line.or_else(|| {
                        (cml && !ril)
                            .then(|| leaf_line_indent.get(ls).copied())
                            .flatten()
                    })
                };
                if let Some(v) = cont_marker_root {
                    return_dot_override = Some(v);
                }
                let elem_bonus = match &leaves[chain_root].anchor {
                    Some(Anchor::Arg(c))
                        if (facts.own_break.contains(&c.lparen)
                            || facts.zero_col_expand.contains(&c.lparen))
                            && !c.enum_const
                            && c.smart
                            && facts.line_no.get(chain_root) == facts.line_no.get(c.lparen) =>
                    {
                        config.continuation_indent
                    }
                    _ => 0,
                };
                let outer_chain_bonus = match &leaves[chain_root].anchor {
                    Some(Anchor::Arg(c))
                        if facts.line_no.get(c.lparen) == facts.line_no.get(chain_root) =>
                    {
                        match c.chain {
                            Some(id)
                                if facts
                                    .chain_first_probe
                                    .get(&id)
                                    .is_some_and(|&fp| c.lparen >= fp)
                                    && facts.chain_dot_breaks.get(&id).is_some_and(|d| {
                                        d.iter().any(|&p| p > c.lparen)
                                            && !d.iter().any(|&p| p <= c.lparen)
                                    }) =>
                            {
                                config.continuation_indent
                            }
                            _ => 0,
                        }
                    }
                    _ => 0,
                };
                if return_dot_override.is_some() {
                    return_dot_override
                } else {
                    let skip_op = {
                        let ls = facts.line_start_of.get(chain_root).copied().unwrap_or(0);
                        is_binary_op_text(&leaves[ls].text)
                            && match &leaves[chain_root].anchor {
                                Some(Anchor::Arg(c)) => {
                                    c.lparen > ls
                                        && facts.line_no.get(c.lparen)
                                            == facts.line_no.get(chain_root)
                                }
                                _ => false,
                            }
                    };
                    let (als, scored) = chain_anchor_line(
                        chain_root,
                        leaves,
                        &facts.line_start_of,
                        leaf_line_indent,
                        config.align_multiline_ternary,
                        skip_op,
                    );
                    let mut ancestor_bonus = 0usize;
                    {
                        let mut p = match &leaves[chain_root].anchor {
                            Some(Anchor::Arg(c)) => c.parent.clone(),
                            _ => None,
                        };
                        while let Some(c) = p {
                            if c.lparen <= als {
                                break;
                            }
                            if facts.own_break.contains(&c.lparen)
                                && !c.enum_const
                                && c.smart
                                && !scored.contains(&c.lparen)
                            {
                                ancestor_bonus += config.continuation_indent;
                            }
                            p = c.parent.clone();
                        }
                    }
                    if debug_dot {
                        eprintln!(
                            "  DOTCALC root={} als={} als_txt={:?} als_ind={:?} scored={:?} elem={} outer={} anc={}",
                            chain_root,
                            als,
                            leaves[als].text,
                            leaf_line_indent.get(als),
                            scored,
                            elem_bonus,
                            outer_chain_bonus,
                            ancestor_bonus
                        );
                    }
                    leaf_line_indent.get(als).map(|&b| {
                        b + (scored.len() + 1) * config.continuation_indent
                            + elem_bonus
                            + outer_chain_bonus
                            + ancestor_bonus
                    })
                }
            }
            Some((_, 2, _)) => {
                Some(visual_width(indent, config.indent_size) + config.continuation_indent)
            }
            _ => None,
        };

        if let Some((chain_root, 1, _)) = leaf.chain_dot {
            let root_line = facts
                .line_start_of
                .get(chain_root)
                .copied()
                .unwrap_or(chain_root);
            let unary_logical_rhs = chain_root > root_line + 1
                && leaves[chain_root.saturating_sub(24)..chain_root]
                    .windows(2)
                    .any(|pair| pair[0].text.trim() == "(" && pair[1].text.trim() == "!")
                && leaves[chain_root.saturating_sub(24)..chain_root]
                    .iter()
                    .filter(|leaf| leaf.text.trim() == "!")
                    .take(2)
                    .count()
                    == 2
                && !matches!(
                    leaves[root_line].text.as_str(),
                    "if" | "while" | "return" | "for" | "switch" | "throw" | "assert" | "yield"
                )
                && leaves[chain_root - 1].text.trim() == "!"
                && leaves[root_line..chain_root - 1]
                    .iter()
                    .any(|entry| matches!(entry.text.trim(), "&&" | "||"))
                && visual_width(indent, config.indent_size)
                    == leaf_line_indent.get(root_line).copied().unwrap_or(0);
            if unary_logical_rhs {
                dot_override = leaf_line_indent.get(root_line).copied();
            }
        }

        if debug_dot && leaf.chain_dot.is_some() {
            eprintln!(
                "DOTOV leaf={:?} override={:?} static={}",
                leaf.text.chars().take(10).collect::<String>(),
                dot_override,
                visual_width(indent, config.indent_size)
            );
        }
        let dot_lower_ok = match leaf.chain_dot {
            Some((cr, _, _)) => {
                let elem_start = cr > 0 && matches!(leaves[cr - 1].text.trim(), "(" | "," | "->");
                let same_line_paren = match &leaves[cr].anchor {
                    Some(Anchor::Arg(c)) => {
                        let outer_chunk_scored = c.chain.is_some_and(|id| {
                            facts
                                .chain_dot_breaks
                                .get(&id)
                                .is_some_and(|d| d.iter().any(|&p| p <= c.lparen))
                        });
                        facts.line_no.get(c.lparen) == facts.line_no.get(cr)
                            && (facts.own_break.contains(&c.lparen) || outer_chunk_scored)
                    }
                    _ => false,
                };
                !(elem_start && same_line_paren)
            }
            None => false,
        };
        ChainIndent {
            columns: dot_override,
            may_lower: dot_lower_ok,
        }
    }
}
