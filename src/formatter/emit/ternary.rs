use super::{LineContext, ancestor_line_start_opt};

#[derive(Clone, Copy)]
pub(super) struct TernaryIndent {
    pub(super) tern_colon_override: Option<usize>,
    pub(super) tern_col_override: Option<usize>,
    pub(super) tern_nest_override: Option<usize>,
    pub(super) tern_nest_indent_part: Option<usize>,
    pub(super) tern_bin_override: Option<(usize, usize)>,
    pub(super) tern_after_sign_override: Option<usize>,
}
impl LineContext<'_> {
    pub(super) fn ternary_indent(&self) -> TernaryIndent {
        let i = self.i;
        let leaves = self.leaves;
        let config = self.config;
        let facts = self.facts;
        let leaf_line_indent = &self.history.line_indent;
        let leaf_line_indent_part = &self.history.indent_part;
        let leaf_start_col = &self.history.start_column;
        let leaf = &self.leaves[self.i];
        let tern_colon_override: Option<usize> = leaf.tern_colon.and_then(|(bf, own)| {
            if let Some(o0) = own {
                let mut o = o0;
                while !facts.starts.get(o).copied().unwrap_or(false)
                    && o > 0
                    && leaves[o - 1].text == "("
                {
                    o -= 1;
                }
                if facts.starts.get(o).copied().unwrap_or(false) {
                    return Some(leaf_line_indent.get(o).copied()? + config.continuation_indent);
                }
            }
            let ls = ancestor_line_start_opt(
                bf,
                leaves,
                &facts.line_start_of,
                leaf_line_indent,
                config.align_multiline_ternary,
                true,
            )
            .0;
            Some(leaf_line_indent.get(ls).copied()? + config.continuation_indent)
        });

        let tern_col_override: Option<usize> = leaf
            .tern_nest_col
            .filter(|&bf| facts.line_start_of.get(bf) == Some(&bf))
            .and_then(|bf| leaf_start_col.get(bf).copied())
            .or_else(|| {
                let bf = leaf
                    .tern_nest
                    .map(|(b, _)| b)
                    .filter(|_| leaf.text.as_str() == "?")?;
                if *facts.line_start_of.get(bf)? == bf {
                    leaf_start_col.get(bf).copied()
                } else {
                    None
                }
            })
            .or_else(|| {
                if !leaf.tern_signcol {
                    return None;
                }
                (0..i).rev().find_map(|j| {
                    let t = leaves[j].text.as_str();
                    if t.len() <= 2 && t.ends_with(':') {
                        Some(leaf_start_col.get(j).copied()? + t.len() - 1)
                    } else {
                        None
                    }
                })
            });

        let tern_nest_pair: Option<(usize, Option<usize>)> =
            leaf.tern_nest.and_then(|(bf, nest_deep)| {
                let ls = *facts.line_start_of.get(bf)?;
                if ls != bf
                    && (leaf.text.as_str() == "?"
                        || (nest_deep && !leaf.text.trim_start().starts_with(':')))
                {
                    return Some((
                        leaf_start_col.get(bf).copied()?,
                        leaf_line_indent.get(ls).copied(),
                    ));
                }
                let ls = ancestor_line_start_opt(
                    bf,
                    leaves,
                    &facts.line_start_of,
                    leaf_line_indent,
                    config.align_multiline_ternary,
                    true,
                )
                .0;
                Some((
                    leaf_line_indent.get(ls).copied()? + config.continuation_indent,
                    None,
                ))
            });
        let tern_nest_override: Option<usize> = tern_nest_pair.map(|(t, _)| t);
        let tern_nest_indent_part: Option<usize> = tern_nest_pair.and_then(|(_, p)| p);

        let tern_bin_override: Option<(usize, usize)> = leaf.tern_bin.and_then(|bf| {
            if bf == i && leaf.tern_branch.is_some() && i > 0 && leaves[i - 1].text.trim() == "?" {
                return Some((
                    leaf_start_col.get(i - 1).copied()?,
                    leaf_line_indent_part.get(i - 1).copied().unwrap_or(0),
                ));
            }
            Some((
                leaf_start_col.get(bf).copied()?,
                leaf_line_indent_part.get(bf).copied().unwrap_or(0),
            ))
        });

        let tern_after_sign_override: Option<usize> = (|| -> Option<usize> {
            if leaf.anchor.is_some() {
                return None;
            }
            let mut j = i;
            loop {
                j = j.checked_sub(1)?;
                let t = leaves[j].text.as_str();
                if (t.starts_with("//") || t.starts_with("/*"))
                    && facts.line_start_of.get(j) != Some(&j)
                {
                    continue;
                }
                if !matches!(t.trim(), "?" | ":")
                    || facts.line_start_of.get(j) != Some(&j)
                    || facts.line_no.get(j) >= facts.line_no.get(i)
                {
                    return None;
                }
                return leaf_line_indent
                    .get(j)
                    .map(|c| c + config.continuation_indent);
            }
        })();
        TernaryIndent {
            tern_colon_override,
            tern_col_override,
            tern_nest_override,
            tern_nest_indent_part,
            tern_bin_override,
            tern_after_sign_override,
        }
    }
}
