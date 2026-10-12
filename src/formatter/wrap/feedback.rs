use super::{Anchor, HashMap, LeafEntry, WrapProcessor};

impl WrapProcessor {
    pub(super) fn refine_indent(
        &self,
        mut col: usize,
        line_start: usize,
        leaves: &[LeafEntry],
    ) -> usize {
        if let Some(o) = self
            .indent_oracle
            .as_ref()
            .map(|oracle| &oracle.full)
            .and_then(|o| o.get(line_start).copied())
            .flatten()
        {
            let bin_line = (leaves[line_start].bin_op.is_some()
                || matches!(
                    leaves[line_start].text.trim(),
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
                ))
                && !matches!(
                    &leaves[line_start].anchor,
                    Some(Anchor::Arg(ctx)) if ctx.bin_suppressed
                )
                && match &leaves[line_start].anchor {
                    Some(Anchor::Arg(ctx)) if ctx.smart => (ctx.lparen + 1..line_start)
                        .any(|j| leaves[j].white_space.contains_line_feeds()),
                    _ => true,
                };
            let lam_line = leaves[line_start].lambda_body_cont.is_some()
                && line_start > 0
                && leaves[line_start - 1].text.trim() == "->";
            let o_scan = self
                .indent_oracle
                .as_ref()
                .map(|oracle| &oracle.scan)
                .and_then(|s| s.get(line_start).copied())
                .flatten()
                .unwrap_or(o);
            let mut late = self.enforce_deduction(line_start, leaves);
            if let Some(Anchor::Arg(c)) = &leaves[line_start].anchor {
                if !c.chain_wrapper_static
                    && leaves[line_start].arg_probe_for == Some(c.lparen)
                    && let Some(sf) = c.seg_first
                {
                    let chain_broke = c.chain.is_some_and(|cr| {
                        (cr + 1..=sf).any(|j| leaves[j].white_space.contains_line_feeds())
                    });
                    if sf < line_start
                        && !leaves[sf].white_space.contains_line_feeds()
                        && chain_broke
                    {
                        late += self.continuation_indent;
                    }
                }
                if c.bin_suppressed
                    && !c.elem_has_cont
                    && (c.lparen + 1..line_start).any(|index| {
                        leaves[index].arg_probe_for == Some(c.lparen)
                            && leaves[index].white_space.contains_line_feeds()
                    })
                {
                    late += self.continuation_indent;
                }
            }
            let chain_has_nested_call = |root: usize| {
                if let Some(result) = self.chain_nested_call.borrow().get(&root).copied() {
                    return result;
                }
                let chain_end = leaves.get(root).map_or(root, |leaf| {
                    leaf.span_end.min(leaves.len().saturating_sub(1))
                });
                let result = (root..=chain_end)
                    .rev()
                    .find(|&index| leaves[index].chain_probe_for == Some(root))
                    .is_some_and(|segment| {
                        if segment + 2 >= leaves.len()
                            || leaves[segment + 1].text != "("
                            || leaves[segment + 2].text == ")"
                        {
                            return false;
                        }
                        let mut depth = 0usize;
                        let mut dots = 0usize;
                        let mut top_level_commas = 0usize;
                        for leaf in &leaves[segment + 1..] {
                            match leaf.text.as_str() {
                                "(" => depth += 1,
                                ")" if depth == 1 => break,
                                ")" => depth = depth.saturating_sub(1),
                                "," if depth == 1 => top_level_commas += 1,
                                _ if leaf.text.trim_start().starts_with('.') => dots += 1,
                                _ => {}
                            }
                        }
                        dots >= 2 && top_level_commas == 0
                    });
                self.chain_nested_call.borrow_mut().insert(root, result);
                result
            };
            let enclosing_body_chain = leaves[line_start].encl_chain.iter().find_map(|&index| {
                (leaves[index].span_end >= line_start)
                    .then_some(leaves[index].chain_probe_for)
                    .flatten()
            });
            let established_body_chain = o >= col
                && o == o_scan + self.continuation_indent
                && match &leaves[line_start].anchor {
                    Some(Anchor::Body(_, _, _, brace)) => {
                        enclosing_body_chain.is_some_and(chain_has_nested_call)
                            && !leaves[line_start].encl_chain.is_empty()
                            && (*brace + 1..line_start)
                                .any(|j| leaves[j].white_space.contains_line_feeds())
                    }
                    Some(Anchor::Arg(ctx)) => {
                        ctx.chain.is_some_and(chain_has_nested_call)
                            && ctx.body.as_ref().is_some_and(|(_, _, brace)| {
                                (*brace + 1..line_start)
                                    .any(|j| leaves[j].white_space.contains_line_feeds())
                            })
                    }
                    None => false,
                };
            let chain_line =
                o > col && (self.chain_body_line(line_start, leaves) || established_body_chain);
            let paren_line = o > col
                && leaves[line_start]
                    .paren_cwf
                    .is_some_and(|lp| lp < line_start);
            let o_scan_r = o_scan.saturating_sub(late);
            col = if established_body_chain {
                o
            } else if o_scan_r > col {
                o_scan_r
            } else if self.indent_oracle.is_some() && !chain_line {
                if o <= col { o_scan } else { col }
            } else if o > col + self.continuation_indent
                || ((bin_line || lam_line || chain_line || paren_line) && o > col)
            {
                o
            } else {
                col.min(o)
            };
        }

        col
    }

    pub(super) fn chain_body_line(&self, line_start: usize, leaves: &[LeafEntry]) -> bool {
        if leaves[line_start].encl_chain.is_empty() {
            return false;
        }
        if self.chain_src_break.borrow().is_none() {
            let mut map: HashMap<usize, Vec<(usize, bool)>> = HashMap::new();
            for (j, l) in leaves.iter().enumerate() {
                if let Some(r) = l.chain_probe_for {
                    map.entry(r)
                        .or_default()
                        .push((j, l.white_space.contains_line_feeds()));
                }
            }
            *self.chain_src_break.borrow_mut() = Some(map);
        }
        let borrowed = self.chain_src_break.borrow();
        let map = borrowed.as_ref().unwrap();
        if leaves[line_start].chain_probe_for.is_some() {
            return false;
        }
        let in_body = match &leaves[line_start].anchor {
            Some(Anchor::Body(..)) => leaves[line_start].text != "}",
            Some(Anchor::Arg(c)) => c.body.is_some(),
            None => false,
        };
        if !in_body {
            return false;
        }
        leaves[line_start].encl_chain.iter().any(|&r| {
            if r == line_start {
                return false;
            }
            let Some(dots) = map.get(&r) else {
                return false;
            };
            let mut ahead = dots.iter().skip_while(|&&(j, _)| j <= line_start);
            match ahead.next() {
                Some(&(_, false)) => ahead.any(|&(_, f)| f),
                _ => false,
            }
        })
    }
}
