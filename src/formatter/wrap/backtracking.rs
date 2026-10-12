use super::{Anchor, LeafEntry, WrapId, WrapProcessor, is_binary_op_text};

impl WrapProcessor {
    pub(super) fn direct_unary_chain_argument(
        &self,
        i: usize,
        fwol: usize,
        leaves: &[LeafEntry],
    ) -> bool {
        i == fwol + 2
            && leaves[i - 1].text.trim() == "("
            && leaves[fwol].text.trim_start().starts_with('.')
            && leaves[fwol].chain_probe_for.is_some_and(|root| {
                root > 0
                    && leaves[root - 1].text.trim() == "!"
                    && leaves[root..=fwol]
                        .iter()
                        .map(|leaf| self.text_width(&leaf.text))
                        .sum::<usize>()
                        > self.right_margin / 3
            })
            && {
                let mut depth = 0usize;
                let mut dots = 0usize;
                for leaf in &leaves[i..] {
                    match leaf.text.as_str() {
                        "(" => depth += 1,
                        ")" if depth == 0 => break,
                        ")" => depth -= 1,
                        _ if leaf.text.trim_start().starts_with('.') => {
                            dots += 1;
                        }
                        _ => {}
                    }
                }
                dots >= 4
            }
            && leaves[fwol].span_end >= i
    }

    pub(super) fn body_argument_chain(&self, i: usize, fwol: usize, leaves: &[LeafEntry]) -> bool {
        leaves[i].text.trim_start().starts_with('.')
            && matches!(
                &leaves[fwol].anchor,
                Some(Anchor::Arg(ctx))
                    if ctx.body.is_some() && ctx.chain.is_some()
            )
            && leaves[fwol]
                .text
                .trim_start()
                .chars()
                .next()
                .is_some_and(char::is_uppercase)
            && i + 2 < leaves.len()
            && leaves[i + 1].text == "("
            && leaves[i + 2].text != ")"
            && {
                let mut depth = 0usize;
                let mut dots = 0usize;
                let mut top_level_commas = 0usize;
                let mut has_lambda = false;
                let mut has_binary_operator = false;
                let mut in_string = false;
                for leaf in &leaves[i + 1..] {
                    match leaf.text.trim() {
                        "\"" => in_string = !in_string,
                        "(" => depth += 1,
                        ")" if depth == 1 => break,
                        ")" => depth = depth.saturating_sub(1),
                        "," if depth == 1 => top_level_commas += 1,
                        "->" => has_lambda = true,
                        "+" | "-" | "*" | "/" | "%" | "&&" | "||" | "==" | "!=" | "<" | ">"
                        | "<=" | ">=" => {
                            has_binary_operator = true;
                        }
                        _ if !in_string && leaf.text.trim_start().starts_with('.') => {
                            dots += 1;
                        }
                        _ => {}
                    }
                }
                dots == 2 && top_level_commas == 0 && !has_lambda && !has_binary_operator
            }
    }

    pub(super) fn near_margin_body_chain(
        &self,
        i: usize,
        cand_idx: usize,
        leaves: &[LeafEntry],
    ) -> bool {
        leaves[cand_idx].text.trim_start().starts_with('.')
            && (matches!(
                &leaves[cand_idx].anchor,
                Some(Anchor::Arg(ctx)) if ctx.body.is_some() && ctx.chain.is_some()
            ) || matches!(&leaves[cand_idx].anchor, Some(Anchor::Body(..))))
            && {
                let line_start = (0..=i)
                    .rev()
                    .find(|&index| index == 0 || leaves[index].white_space.contains_line_feeds())
                    .unwrap_or(0);
                self.indent_oracle
                    .as_ref()
                    .map(|oracle| &oracle.full)
                    .and_then(|oracle| oracle.get(line_start))
                    .copied()
                    .flatten()
                    .is_some_and(|indent| {
                        let mut end = indent;
                        for (offset, leaf) in leaves[line_start..=i].iter().enumerate() {
                            if offset > 0 {
                                end += leaf.white_space.spaces;
                            }
                            end += self.text_width(&leaf.text);
                        }
                        end > self.right_margin
                    })
            }
            && leaves
                .get(cand_idx + 1)
                .is_some_and(|leaf| leaf.text == "(")
            && {
                let mut depth = 0usize;
                let mut dots = 0usize;
                let mut commas = 0usize;
                let mut disallowed = false;
                let mut in_string = false;
                for leaf in &leaves[cand_idx + 1..] {
                    match leaf.text.trim() {
                        "\"" => in_string = !in_string,
                        "(" => depth += 1,
                        ")" if depth == 1 => break,
                        ")" => depth = depth.saturating_sub(1),
                        "," if depth == 1 => commas += 1,
                        "->" | "+" | "-" | "*" | "/" | "%" | "&&" | "||" | "==" | "!=" | "<"
                        | ">" | "<=" | ">=" => {
                            disallowed = true;
                        }
                        _ if !in_string && leaf.text.trim_start().starts_with('.') => {
                            dots += 1;
                        }
                        _ => {}
                    }
                }
                dots == 2 && commas == 0 && !disallowed
            }
    }

    pub(super) fn has_later_binary_member(
        &self,
        i: usize,
        cand_idx: usize,
        cand_wid: WrapId,
        leaves: &[LeafEntry],
        initial_feeds: &[bool],
    ) -> bool {
        let overflow_started_at_string_delimiter = leaves[i].text == "\""
            || (i > 0 && leaves[i - 1].text == "\"" && self.line_over(i - 1, leaves));
        let whitespace_string_literal = (leaves[i].text == "\""
            && leaves
                .get(i + 1)
                .is_some_and(|body| body.text.trim().is_empty())
            && leaves.get(i + 2).is_some_and(|quote| quote.text == "\""))
            || (leaves[i].text.trim().is_empty()
                && i > 0
                && leaves[i - 1].text == "\""
                && leaves.get(i + 1).is_some_and(|quote| quote.text == "\""))
            || (leaves[i].text == "\""
                && i >= 2
                && leaves[i - 2].text == "\""
                && leaves[i - 1].text.trim().is_empty());
        overflow_started_at_string_delimiter
            && whitespace_string_literal
            && leaves[cand_idx].bin_first.is_some()
            && is_binary_op_text(&leaves[cand_idx].text)
            && leaves[i + 1..]
                .iter()
                .enumerate()
                .take_while(|(_, leaf)| !leaf.white_space.contains_line_feeds())
                .any(|(offset, leaf)| {
                    leaf.bin_first == leaves[cand_idx].bin_first
                        && is_binary_op_text(&leaf.text)
                        && leaf.wraps.contains(&cand_wid)
                        && leaves.get(i + offset + 2).is_some_and(|next| {
                            next.text == "(" && !next.source_has_line_break_before && {
                                let paren = i + offset + 2;
                                let mut depth = 0usize;
                                let mut source_break_inside = false;
                                for (inner_offset, entry) in leaves[paren..].iter().enumerate() {
                                    match entry.text.as_str() {
                                        "(" => depth += 1,
                                        ")" => {
                                            depth = depth.saturating_sub(1);
                                            if depth == 0 {
                                                break;
                                            }
                                        }
                                        _ if depth > 0 && initial_feeds[paren + inner_offset] => {
                                            source_break_inside = true;
                                        }
                                        _ => {}
                                    }
                                }
                                source_break_inside
                            }
                        })
                })
    }
}
