use super::*;

impl<'a> BlockBuilder<'a> {
    pub(super) fn merged_anonymous_initializer(
        &mut self,
        all_body_children: &[Node],
    ) -> Option<Block> {
        let non_structural: Vec<&Node> = all_body_children
            .iter()
            .filter(|c| !matches!(c.kind(), "{" | "}" | ";" | ",") && !c.is_extra())
            .collect();
        if non_structural.len() == 1 && non_structural[0].kind() == "block" {
            let block_node = *non_structural[0];
            let outer_open = all_body_children.iter().find(|n| n.kind() == "{");
            let inner_open = {
                let mut c = block_node.walk();
                block_node.children(&mut c).find(|n| n.kind() == "{")
            };
            let source_merged = match (outer_open, inner_open) {
                (Some(o), Some(i)) => o.end_position().row == i.start_position().row,
                _ => false,
            };
            if source_merged {
                let mut result_children = Vec::new();
                result_children.push(Block::leaf("{{"));

                let mut bc = block_node.walk();
                let block_kids: Vec<Node> = block_node.children(&mut bc).collect();
                let mut inner_prev_row: Option<usize> = None;
                let mut inner_prev_kind: Option<String> = None;
                let mut prev_end_byte: Option<usize> = None;

                for bk in &block_kids {
                    if bk.kind() == "{" {
                        inner_prev_row = Some(bk.end_position().row);
                        inner_prev_kind = Some("{".to_string());
                        prev_end_byte = Some(bk.end_byte());
                        continue;
                    }
                    if bk.kind() == "}" {
                        continue;
                    }

                    let inner_block = if bk.is_extra() {
                        self.comment_leaf(*bk).with_indent(IndentType::Normal)
                    } else {
                        self.node(*bk).with_indent(IndentType::Normal)
                    };

                    let sp = if let Some(pr) = inner_prev_row {
                        let rg = bk.start_position().row.saturating_sub(pr);
                        if rg >= 2 {
                            let capped = rg.min(self.cfg.keep_blank_lines_in_code + 1);
                            Spacing {
                                min_spaces: 0,
                                max_spaces: 0,
                                min_line_feeds: capped as u32,
                                keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                                keep_line_breaks: true,
                            }
                        } else if rg == 0
                            && bk.is_extra()
                            && inner_prev_kind.as_deref() != Some("{")
                        {
                            let pb = prev_end_byte.unwrap_or(0);
                            let ge = bk.start_byte().min(self.src.len());
                            let ts = self.cfg.indent_size.max(1) as u32;
                            let spaces = if pb < ge {
                                self.src[pb..ge]
                                    .iter()
                                    .map(|&b| {
                                        if b == b'\t' {
                                            ts
                                        } else if b == b' ' {
                                            1
                                        } else {
                                            0
                                        }
                                    })
                                    .sum::<u32>()
                            } else {
                                0
                            };
                            Spacing::spaces(spaces, spaces)
                        } else {
                            Spacing::new_line()
                        }
                    } else {
                        Spacing::none()
                    };

                    result_children.push(inner_block.with_spacing(sp));
                    inner_prev_row = Some(bk.end_position().row);
                    inner_prev_kind = Some(bk.kind().to_string());
                    prev_end_byte = Some(bk.end_byte());
                }

                let close_sp = if let Some(pr) = inner_prev_row {
                    let close_node = all_body_children.iter().rev().find(|c| c.kind() == "}");
                    if let Some(cn) = close_node {
                        let rg = cn.start_position().row.saturating_sub(pr);
                        let capped = rg.min(self.cfg.keep_blank_lines_before_rbrace + 1).max(1);
                        Spacing {
                            min_spaces: 0,
                            max_spaces: 0,
                            min_line_feeds: capped as u32,
                            keep_blank_lines: self.cfg.keep_blank_lines_before_rbrace as u32,
                            keep_line_breaks: true,
                        }
                    } else {
                        Spacing::new_line()
                    }
                } else {
                    Spacing::new_line()
                };
                result_children.push(Block::leaf("}}").with_spacing(close_sp));

                return Some(Block::node(result_children));
            }
        }
        None
    }

    pub(super) fn enum_body_members(
        &mut self,
        child: Node,
        member_spacing: MemberSpacing,
    ) -> Block {
        let blank_lines_for = |category| member_spacing.blank_lines(category);
        let mut ebd_children = Vec::new();
        let mut ebd_prev_row: Option<usize> = None;
        let mut ebd_prev_kind: Option<String> = None;
        let mut ebd_prev_member_cat: Option<MemberCategory> = None;

        let ebd_all: Vec<Node> = {
            let mut ec = child.walk();
            child.children(&mut ec).collect()
        };
        let (ebd_field_align, ebd_cont_align) = if self.cfg.align_group_field_declarations {
            let ebd_members: Vec<Node> = ebd_all
                .iter()
                .filter(|c| !matches!(c.kind(), ";" | "{" | "}" | ","))
                .copied()
                .collect();
            self.compute_field_group_alignment(&ebd_members)
        } else {
            (HashMap::new(), HashMap::new())
        };

        let ebd_kids: Vec<Node> = {
            let mut c2 = child.walk();
            child.children(&mut c2).collect()
        };
        let mut ebd_prev_end_byte: Option<usize> = None;
        for (ebd_idx, &ebd_child) in ebd_kids.iter().enumerate() {
            let b = if ebd_child.kind() == ";" {
                Block::leaf(";").with_indent(IndentType::Normal)
            } else if ebd_child.is_extra() {
                self.comment_leaf(ebd_child).with_indent(IndentType::Normal)
            } else if matches!(
                ebd_child.kind(),
                "field_declaration" | "constant_declaration"
            ) {
                if let Some(&(mp, tp, np)) = ebd_field_align.get(&ebd_child.id()) {
                    self.field_declaration_aligned(ebd_child, mp, tp, np, &ebd_cont_align)
                        .with_indent(IndentType::Normal)
                } else {
                    self.node(ebd_child).with_indent(IndentType::Normal)
                }
            } else {
                self.node(ebd_child).with_indent(IndentType::Normal)
            };
            let sp = if let Some(pr) = ebd_prev_row {
                let rg = ebd_child.start_position().row.saturating_sub(pr);
                if ebd_child.kind() == ";" {
                    let required_blanks = ebd_prev_member_cat.map(&blank_lines_for).unwrap_or(0);
                    let min_feeds = if required_blanks > 0 {
                        (1 + required_blanks) as u32
                    } else {
                        1
                    };
                    let preserved = if rg >= 2 {
                        rg.min(self.cfg.keep_blank_lines_in_declarations + 1) as u32
                    } else {
                        0
                    };
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: min_feeds.max(preserved),
                        keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                        keep_line_breaks: true,
                    }
                } else {
                    let ebd_cur_cat = MemberCategory::from_kind(ebd_child.kind());
                    let ebd_prev_is_comment = ebd_prev_kind
                        .as_deref()
                        .is_some_and(|k| k == "line_comment" || k == "block_comment");
                    let required_blanks = if ebd_prev_is_comment {
                        let ebd_prev_is_method_like = matches!(
                            ebd_prev_member_cat,
                            Some(MemberCategory::Method | MemberCategory::Class)
                        );
                        if ebd_prev_is_method_like
                            && ebd_prev_kind.as_deref() == Some("line_comment")
                            && ebd_child.kind() == "block_comment"
                            && self.text(ebd_child).starts_with("/**")
                            && !self.text(ebd_child).starts_with("/***")
                        {
                            1
                        } else {
                            0
                        }
                    } else {
                        match (ebd_prev_member_cat, ebd_cur_cat) {
                            (Some(prev), Some(cur)) => {
                                blank_lines_for(prev).max(blank_lines_for(cur))
                            }
                            (Some(prev), None) if ebd_child.is_extra() => {
                                let ebd_rg = ebd_child
                                    .start_position()
                                    .row
                                    .saturating_sub(ebd_prev_row.unwrap_or(0));
                                let is_javadoc = ebd_child.kind() == "block_comment"
                                    && self.text(ebd_child).starts_with("/**")
                                    && !self.text(ebd_child).starts_with("/***");
                                let leads_member = !is_javadoc && ebd_rg < 2 && {
                                    let mut last_row = ebd_child.end_position().row;
                                    let mut found = false;
                                    for &sib in &ebd_kids[(ebd_idx + 1)..] {
                                        let gap = sib.start_position().row.saturating_sub(last_row);
                                        if gap >= 2 {
                                            break;
                                        }
                                        if sib.is_extra() {
                                            let t = self.text(sib);
                                            if sib.kind() == "block_comment"
                                                && t.starts_with("/**")
                                                && !t.starts_with("/***")
                                            {
                                                break;
                                            }
                                            last_row = sib.end_position().row;
                                            continue;
                                        }
                                        found = MemberCategory::from_kind(sib.kind()).is_some();
                                        break;
                                    }
                                    found
                                };
                                if ebd_rg >= 2 || is_javadoc || leads_member {
                                    blank_lines_for(prev)
                                } else {
                                    0
                                }
                            }
                            (None, Some(cur)) => blank_lines_for(cur),
                            _ => 0,
                        }
                    };
                    let min_feeds = if required_blanks > 0 {
                        (1 + required_blanks) as u32
                    } else if rg >= 1 {
                        1
                    } else {
                        0
                    };
                    let preserved = if rg >= 2 {
                        let raw = rg.min(self.cfg.keep_blank_lines_in_declarations + 1) as u32;
                        let cur_is_javadoc = ebd_child.kind() == "block_comment"
                            && self.text(ebd_child).starts_with("/**")
                            && !self.text(ebd_child).starts_with("/***");
                        if ebd_prev_is_comment
                            && required_blanks == 0
                            && ebd_child.is_extra()
                            && !cur_is_javadoc
                        {
                            0
                        } else {
                            raw
                        }
                    } else {
                        0
                    };
                    let final_feeds = min_feeds.max(preserved);
                    if final_feeds == 0 {
                        if ebd_child.is_extra() {
                            let pb = ebd_prev_end_byte.unwrap_or(0);
                            let ge = ebd_child.start_byte().min(self.src.len());
                            let spaces = if pb < ge {
                                self.src[pb..ge]
                                    .iter()
                                    .map(|&b| {
                                        if b == b'\t' {
                                            self.cfg.indent_size.max(1) as u32
                                        } else if b == b' ' {
                                            1
                                        } else {
                                            0
                                        }
                                    })
                                    .sum::<u32>()
                                    .max(1)
                            } else {
                                1
                            };
                            Spacing::spaces(spaces, spaces)
                        } else {
                            Spacing::one_space()
                        }
                    } else {
                        Spacing {
                            min_spaces: 0,
                            max_spaces: 0,
                            min_line_feeds: final_feeds,
                            keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                            keep_line_breaks: true,
                        }
                    }
                }
            } else {
                Spacing::none()
            };
            let b = if ebd_prev_row.is_some() {
                b.with_spacing(sp)
            } else {
                b
            };
            if let Some(cat) = MemberCategory::from_kind(ebd_child.kind()) {
                ebd_prev_member_cat = Some(cat);
            }
            ebd_prev_kind = Some(ebd_child.kind().to_string());
            ebd_prev_row = Some(ebd_child.end_position().row);
            ebd_prev_end_byte = Some(ebd_child.end_byte());
            ebd_children.push(b);
        }
        Block::node(ebd_children)
    }
}
