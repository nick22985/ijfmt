use super::BlockBuilder;
use crate::config::ImportGroup;
use crate::formatter::ir::*;

pub(crate) struct ImportEntry {
    pub text: String,
    pub source_gap: usize,
    pub end_row: usize,
    pub trailing_comment: Option<String>,
    pub leading_comments: Vec<String>,
    pub leading_comment_feeds: Option<usize>,
}

struct ParsedImport {
    is_static: bool,
    path: String,
    source_gap: usize,
    trailing_comment: Option<String>,
    leading_comments: Vec<String>,
    leading_comment_feeds: Option<usize>,
    extra_semicolons: usize,
}

impl ParsedImport {
    fn from_entry(entry: &ImportEntry) -> Self {
        let trimmed = entry.text.trim();
        let extra_semicolons = trimmed.len() - trimmed.trim_end_matches(';').len();
        let extra_semicolons = extra_semicolons.saturating_sub(1);
        let is_static = trimmed.starts_with("import static ");
        let path = if is_static {
            trimmed
                .trim_start_matches("import static ")
                .trim_end_matches(';')
                .trim()
                .to_owned()
        } else {
            trimmed
                .trim_start_matches("import ")
                .trim_end_matches(';')
                .trim()
                .to_owned()
        };
        ParsedImport {
            is_static,
            path,
            source_gap: entry.source_gap,
            trailing_comment: entry.trailing_comment.clone(),
            leading_comments: entry.leading_comments.clone(),
            leading_comment_feeds: entry.leading_comment_feeds,
            extra_semicolons,
        }
    }

    fn to_import_line(&self) -> String {
        let tail = ";".repeat(1 + self.extra_semicolons);
        if self.is_static {
            format!("import static {}{}", self.path, tail)
        } else {
            format!("import {}{}", self.path, tail)
        }
    }
}

impl<'a> BlockBuilder<'a> {
    pub(crate) fn build_sorted_import_blocks(&self, entries: &[ImportEntry]) -> Vec<Block> {
        let parsed: Vec<ParsedImport> = entries.iter().map(ParsedImport::from_entry).collect();

        let is_group_by_top_level = self
            .cfg
            .import_layout
            .iter()
            .any(|g| matches!(g, ImportGroup::GroupByTopLevel));

        if is_group_by_top_level {
            self.build_imports_group_by_top_level(&parsed)
        } else {
            let has_source_blanks = parsed.iter().any(|p| p.source_gap > 0);
            if has_source_blanks {
                self.build_imports_source_preserve(&parsed)
            } else {
                self.build_imports_layout_table(&parsed)
            }
        }
    }

    fn build_imports_group_by_top_level(&self, parsed: &[ParsedImport]) -> Vec<Block> {
        fn is_java_import(path: &str) -> bool {
            path == "java"
                || path.starts_with("java.")
                || path == "javax"
                || path.starts_with("javax.")
        }

        let category_of = |imp: &ParsedImport| -> u8 {
            if imp.is_static {
                2
            } else if is_java_import(&imp.path) {
                1
            } else {
                0
            }
        };

        let keep_blanks = self.cfg.keep_blank_lines_in_declarations;
        let mut blocks = Vec::new();
        let mut prev_category: Option<u8> = None;

        for imp in parsed {
            let cat = category_of(imp);

            let spacing = if let Some(pc) = prev_category {
                if imp.source_gap > 0 || (cat != pc && imp.leading_comments.is_empty()) {
                    let blanks = imp.source_gap.min(keep_blanks).max(1);
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: (blanks + 1) as u32,
                        keep_blank_lines: keep_blanks as u32,
                        keep_line_breaks: true,
                    }
                } else {
                    Spacing::new_line()
                }
            } else {
                Spacing::new_line()
            };

            for (ci, lc) in imp.leading_comments.iter().enumerate() {
                let lc_spacing = if ci == 0 {
                    spacing.clone()
                } else {
                    Spacing::new_line()
                };
                blocks.push(
                    Block::leaf(lc.clone())
                        .with_spacing(lc_spacing)
                        .with_source_line_break(),
                );
            }

            let import_spacing = if !imp.leading_comments.is_empty() {
                after_leading_comment_spacing(imp)
            } else {
                spacing
            };
            let leaf_text = make_import_leaf_text(imp);
            let block = Block::leaf(leaf_text)
                .with_spacing(import_spacing)
                .with_source_line_break();

            blocks.push(block);
            prev_category = Some(cat);
        }

        blocks
    }

    fn build_imports_source_preserve(&self, parsed: &[ParsedImport]) -> Vec<Block> {
        let keep_blanks = self.cfg.keep_blank_lines_in_declarations;

        let mut subgroups: Vec<(usize, Vec<&ParsedImport>)> = Vec::new();
        let mut current: Vec<&ParsedImport> = Vec::new();
        let mut current_gap: usize = 0;
        let mut prev_is_static: Option<bool> = None;

        for imp in parsed {
            let split_for_gap = imp.source_gap > 0 && !current.is_empty();
            let split_for_static = prev_is_static
                .map(|ps| ps != imp.is_static)
                .unwrap_or(false)
                && !current.is_empty();

            if split_for_gap || split_for_static {
                subgroups.push((current_gap, std::mem::take(&mut current)));
                current_gap = if split_for_gap { imp.source_gap } else { 1 };
            }
            current.push(imp);
            prev_is_static = Some(imp.is_static);
        }
        if !current.is_empty() {
            subgroups.push((current_gap, current));
        }

        let mut blocks = Vec::new();
        let mut first_subgroup = true;

        for (gap, subgroup) in &subgroups {
            for (i, imp) in subgroup.iter().enumerate() {
                let spacing = if i == 0 && !first_subgroup {
                    let blanks = (*gap).max(1).min(keep_blanks);
                    Spacing {
                        min_spaces: 0,
                        max_spaces: 0,
                        min_line_feeds: (blanks + 1) as u32,
                        keep_blank_lines: keep_blanks as u32,
                        keep_line_breaks: true,
                    }
                } else {
                    Spacing::new_line()
                };

                for (ci, lc) in imp.leading_comments.iter().enumerate() {
                    let lc_spacing = if ci == 0 {
                        spacing.clone()
                    } else {
                        Spacing::new_line()
                    };
                    blocks.push(
                        Block::leaf(lc.clone())
                            .with_spacing(lc_spacing)
                            .with_source_line_break(),
                    );
                }
                let import_spacing = if !imp.leading_comments.is_empty() {
                    after_leading_comment_spacing(imp)
                } else {
                    spacing
                };
                let leaf_text = make_import_leaf_text(imp);
                let block = Block::leaf(leaf_text)
                    .with_spacing(import_spacing)
                    .with_source_line_break();
                blocks.push(block);
            }
            first_subgroup = false;
        }

        blocks
    }

    fn build_imports_layout_table(&self, parsed: &[ParsedImport]) -> Vec<Block> {
        let layout = &self.cfg.import_layout;
        let keep_blanks = self.cfg.keep_blank_lines_in_declarations;

        let groups: Vec<(usize, &str, bool)> = layout
            .iter()
            .enumerate()
            .filter_map(|(i, g)| match g {
                ImportGroup::Package { prefix, is_static } => {
                    Some((i, prefix.as_str(), *is_static))
                }
                ImportGroup::EmptyLine | ImportGroup::GroupByTopLevel => None,
            })
            .collect();

        let mut assignment: Vec<Option<usize>> = vec![None; parsed.len()];
        for (imp_idx, imp) in parsed.iter().enumerate() {
            let mut best_layout_idx: Option<usize> = None;
            let mut best_prefix_len: usize = 0;
            let mut best_is_specific = false;

            for &(layout_idx, prefix, is_static) in &groups {
                if is_static != imp.is_static {
                    continue;
                }
                let matches = if prefix.is_empty() {
                    true
                } else {
                    imp.path == prefix || imp.path.starts_with(&format!("{}.", prefix))
                };
                if matches {
                    let is_specific = !prefix.is_empty();
                    let prefix_len = prefix.len();
                    if is_specific && prefix_len >= best_prefix_len {
                        best_prefix_len = prefix_len;
                        best_layout_idx = Some(layout_idx);
                        best_is_specific = true;
                    } else if !is_specific && !best_is_specific && best_layout_idx.is_none() {
                        best_layout_idx = Some(layout_idx);
                    }
                }
            }
            assignment[imp_idx] = best_layout_idx;
        }

        let mut emitted = vec![false; parsed.len()];
        let mut blocks = Vec::new();
        let mut pending_blank = false;
        let mut any_output = false;

        for (layout_idx, entry) in layout.iter().enumerate() {
            match entry {
                ImportGroup::EmptyLine => {
                    if any_output {
                        pending_blank = true;
                    }
                }
                ImportGroup::GroupByTopLevel => {}
                ImportGroup::Package { .. } => {
                    let group_imports: Vec<usize> = parsed
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| assignment[*i] == Some(layout_idx) && !emitted[*i])
                        .map(|(i, _)| i)
                        .collect();

                    for imp_idx in group_imports {
                        let imp = &parsed[imp_idx];
                        let spacing = if pending_blank {
                            pending_blank = false;
                            Spacing {
                                min_spaces: 0,
                                max_spaces: 0,
                                min_line_feeds: 2,
                                keep_blank_lines: keep_blanks as u32,
                                keep_line_breaks: true,
                            }
                        } else if imp.source_gap > 0 && any_output {
                            let blanks = imp.source_gap.max(1).min(keep_blanks);
                            Spacing {
                                min_spaces: 0,
                                max_spaces: 0,
                                min_line_feeds: (blanks + 1) as u32,
                                keep_blank_lines: keep_blanks as u32,
                                keep_line_breaks: true,
                            }
                        } else {
                            Spacing::new_line()
                        };

                        for (ci, lc) in imp.leading_comments.iter().enumerate() {
                            let lc_spacing = if ci == 0 {
                                spacing.clone()
                            } else {
                                Spacing::new_line()
                            };
                            blocks.push(
                                Block::leaf(lc.clone())
                                    .with_spacing(lc_spacing)
                                    .with_source_line_break(),
                            );
                        }
                        let import_spacing = if !imp.leading_comments.is_empty() {
                            after_leading_comment_spacing(imp)
                        } else {
                            spacing
                        };
                        let leaf_text = make_import_leaf_text(imp);
                        let block = Block::leaf(leaf_text)
                            .with_spacing(import_spacing)
                            .with_source_line_break();
                        blocks.push(block);
                        emitted[imp_idx] = true;
                        any_output = true;
                    }
                }
            }
        }

        let mut leftover: Vec<usize> = parsed
            .iter()
            .enumerate()
            .filter(|(i, _)| !emitted[*i])
            .map(|(i, _)| i)
            .collect();
        leftover.sort_by_key(|&i| &parsed[i].path);
        for imp_idx in leftover {
            let imp = &parsed[imp_idx];
            for lc in &imp.leading_comments {
                blocks.push(
                    Block::leaf(lc.clone())
                        .with_spacing(Spacing::new_line())
                        .with_source_line_break(),
                );
            }
            let leaf_text = make_import_leaf_text(imp);
            let block = Block::leaf(leaf_text)
                .with_spacing(Spacing::new_line())
                .with_source_line_break();
            blocks.push(block);
        }

        blocks
    }
}

fn after_leading_comment_spacing(imp: &ParsedImport) -> Spacing {
    match imp.leading_comment_feeds {
        Some(feeds) if feeds > 1 => Spacing {
            min_spaces: 0,
            max_spaces: 0,
            min_line_feeds: feeds as u32,
            keep_blank_lines: feeds as u32,
            keep_line_breaks: true,
        },
        _ => Spacing::new_line(),
    }
}

fn make_import_leaf_text(imp: &ParsedImport) -> String {
    let line = imp.to_import_line();
    if let Some(ref comment) = imp.trailing_comment {
        format!("{} {}", line, comment)
    } else {
        line
    }
}
