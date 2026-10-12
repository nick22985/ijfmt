use tree_sitter::Node;

use super::super::ir::*;
use super::BlockBuilder;
use super::imports;

impl<'a> BlockBuilder<'a> {
    pub(super) fn program(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let all_children: Vec<Node> = node.children(&mut cursor).collect();

        let mut pending_imports: Vec<imports::ImportEntry> = Vec::new();
        let mut import_block_emitted = false;
        let mut prev_end_row: Option<usize> = None;
        let mut last_import_end_row: usize = 0;
        let mut prev_kind: Option<String> = None;
        let mut pending_import_comments: Vec<String> = Vec::new();
        let mut first_pending_comment_row: Option<usize> = None;
        let mut last_c_style_comment_end_row: Option<usize> = None;

        for (child_idx, child) in all_children.iter().enumerate() {
            if child.is_extra() && matches!(self.md_run_reflowed(*child), Some((_, i)) if i > 0) {
                prev_end_row = Some(child.end_position().row);
                continue;
            }

            let is_import = !child.is_extra() && child.kind() == "import_declaration";

            if is_import {
                if pending_imports.is_empty() && import_block_emitted {
                    import_block_emitted = false;
                }
                let gap_target_row =
                    first_pending_comment_row.unwrap_or(child.start_position().row);
                let source_gap = if let Some(prev) = pending_imports.last() {
                    gap_target_row.saturating_sub(prev.end_row + 1)
                } else {
                    0
                };
                let text = self.text(*child).trim().to_string();
                let leading = std::mem::take(&mut pending_import_comments);
                first_pending_comment_row = None;
                let leading_comment_feeds = last_c_style_comment_end_row
                    .take()
                    .map(|r| child.start_position().row.saturating_sub(r));
                pending_imports.push(imports::ImportEntry {
                    text,
                    source_gap,
                    end_row: child.end_position().row,
                    trailing_comment: None,
                    leading_comments: leading,
                    leading_comment_feeds,
                });
                last_import_end_row = child.end_position().row;
                continue;
            }

            if child.kind() == ";"
                && !pending_imports.is_empty()
                && !import_block_emitted
                && child.start_position().row == last_import_end_row
            {
                if let Some(last) = pending_imports.last_mut() {
                    last.text.push(';');
                }
                continue;
            }

            if child.is_extra()
                && child.kind() == "line_comment"
                && !pending_imports.is_empty()
                && !import_block_emitted
                && child.start_position().row == last_import_end_row
            {
                let comment_text = self.text(*child).to_string();
                if let Some(last) = pending_imports.last_mut() {
                    last.trailing_comment = Some(comment_text);
                }
                last_import_end_row = child.end_position().row;
                continue;
            }

            if child.is_extra() && !pending_imports.is_empty() && !import_block_emitted {
                let more_imports = all_children[(child_idx + 1)..]
                    .iter()
                    .any(|c| c.kind() == "import_declaration");
                if more_imports {
                    let comment_text = self.text(*child).to_string();
                    if first_pending_comment_row.is_none() {
                        first_pending_comment_row = Some(child.start_position().row);
                    }
                    last_c_style_comment_end_row =
                        if comment_text.starts_with("/*") && !comment_text.starts_with("/**") {
                            Some(child.end_position().row)
                        } else {
                            None
                        };
                    pending_import_comments.push(comment_text);
                    continue;
                }
            }

            if !pending_imports.is_empty() && !import_block_emitted {
                let first_import_row = all_children
                    .iter()
                    .find(|c| c.kind() == "import_declaration")
                    .map(|c| c.start_position().row)
                    .unwrap_or(0);
                let pre_import_gap = prev_end_row
                    .map(|pr| first_import_row.saturating_sub(pr))
                    .unwrap_or(0);

                let import_blocks = self.build_sorted_import_blocks(&pending_imports);
                for (idx, mut ib) in import_blocks.into_iter().enumerate() {
                    if children.is_empty() {
                    } else if idx == 0 {
                        let min_before = self.cfg.blank_lines_before_imports + 1;
                        let max_before = self.cfg.keep_blank_lines_in_declarations + 1;
                        let feeds = pre_import_gap.max(min_before).min(max_before);
                        ib = ib.with_spacing(Spacing {
                            min_spaces: 0,
                            max_spaces: 0,
                            min_line_feeds: feeds as u32,
                            keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                            keep_line_breaks: true,
                        });
                    } else if ib.spacing.is_none() {
                        ib = ib
                            .with_spacing(Spacing::new_line())
                            .with_source_line_break();
                    }
                    children.push(ib);
                }
                prev_end_row = Some(last_import_end_row);
                pending_imports.clear();
                import_block_emitted = true;
            }

            let just_after_imports =
                import_block_emitted && prev_end_row == Some(last_import_end_row);

            let mut block = if child.is_extra() {
                self.comment_leaf(*child)
            } else {
                self.node(*child)
            };

            if let Some(prev_row) = prev_end_row {
                let row_gap = child.start_position().row.saturating_sub(prev_row);
                let is_post_import_comment = just_after_imports
                    && child.is_extra()
                    && child.kind() == "line_comment"
                    && row_gap <= 1;
                let mut min_feeds = if just_after_imports
                    && self.cfg.blank_lines_after_imports > 0
                    && !is_post_import_comment
                {
                    (self.cfg.blank_lines_after_imports + 1) as u32
                } else {
                    0
                };

                if prev_kind.as_deref() == Some("line_comment")
                    && child.is_extra()
                    && child.kind() == "block_comment"
                {
                    let text = self.text(*child);
                    let is_javadoc = text.starts_with("/**") && !text.starts_with("/***");
                    if is_javadoc {
                        let leads_class = all_children[(child_idx + 1)..]
                            .iter()
                            .find(|c| !c.is_extra())
                            .is_some_and(|c| {
                                matches!(
                                    c.kind(),
                                    "class_declaration"
                                        | "interface_declaration"
                                        | "enum_declaration"
                                        | "record_declaration"
                                        | "annotation_type_declaration"
                                )
                            });
                        if leads_class {
                            min_feeds = min_feeds.max(2);
                        }
                    }
                }

                if row_gap >= 2 || min_feeds > 1 {
                    let both_line_comments = prev_kind.as_deref() == Some("line_comment")
                        && child.is_extra()
                        && child.kind() == "line_comment";
                    let max_blanks = if both_line_comments {
                        self.cfg.keep_blank_lines_in_code
                    } else {
                        self.cfg.keep_blank_lines_in_declarations
                    };
                    let max_feeds = (max_blanks + 1) as u32;
                    let feeds = (row_gap as u32).max(min_feeds).min(max_feeds);
                    block = block.with_spacing(Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: feeds,
                        keep_blank_lines: max_blanks as u32,
                        keep_line_breaks: true,
                    });
                } else if row_gap == 1 {
                    block = block
                        .with_spacing(Spacing::new_line())
                        .with_source_line_break();
                } else if child.kind() == ";" {
                    block = block.with_spacing(Spacing::none());
                } else {
                    block = block.with_spacing(Spacing::one_space());
                }
            }

            prev_end_row = Some(child.end_position().row);
            prev_kind = Some(child.kind().to_string());
            children.push(block);
        }

        if !pending_imports.is_empty() && !import_block_emitted {
            let first_import_row = all_children
                .iter()
                .find(|c| c.kind() == "import_declaration")
                .map(|c| c.start_position().row)
                .unwrap_or(0);
            let pre_import_gap = prev_end_row
                .map(|pr| first_import_row.saturating_sub(pr))
                .unwrap_or(0);

            let import_blocks = self.build_sorted_import_blocks(&pending_imports);
            for (idx, mut ib) in import_blocks.into_iter().enumerate() {
                if children.is_empty() {
                } else if idx == 0 {
                    let feeds = pre_import_gap.max(1);
                    ib = ib.with_spacing(Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: feeds as u32,
                        keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                        keep_line_breaks: true,
                    });
                } else if ib.spacing.is_none() {
                    ib = ib
                        .with_spacing(Spacing::new_line())
                        .with_source_line_break();
                }
                children.push(ib);
            }
        }

        Block::node(children)
    }
}
