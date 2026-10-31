use super::*;

#[derive(Clone, Copy)]
pub(super) enum MemberCategory {
    Field,
    Method,
    Class,
    EnumConstant,
    StraySemicolon,
}

impl MemberCategory {
    pub(super) fn from_kind(kind: &str) -> Option<Self> {
        match kind {
            "field_declaration" | "constant_declaration" => Some(Self::Field),
            "method_declaration"
            | "constructor_declaration"
            | "compact_constructor_declaration"
            | "static_initializer"
            | "block" => Some(Self::Method),
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => Some(Self::Class),
            "enum_constant" => Some(Self::EnumConstant),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct MemberSpacing {
    field: usize,
    method: usize,
    class: usize,
}

impl MemberSpacing {
    pub(super) fn new(cfg: &crate::config::StyleConfig, is_interface: bool) -> Self {
        Self {
            field: if is_interface {
                cfg.blank_lines_around_field_in_interface
            } else {
                cfg.blank_lines_around_field
            },
            method: if is_interface {
                cfg.blank_lines_around_method_in_interface
            } else {
                cfg.blank_lines_around_method
            },
            class: cfg.blank_lines_around_class,
        }
    }

    pub(super) fn blank_lines(self, category: MemberCategory) -> usize {
        match category {
            MemberCategory::Field => self.field,
            MemberCategory::Method => self.method,
            MemberCategory::Class => self.class,
            MemberCategory::EnumConstant | MemberCategory::StraySemicolon => 0,
        }
    }
}

pub(super) fn simple_enum_body(node: Node, all_body_children: &[Node]) -> bool {
    if node.kind() == "enum_body" {
        let open_row = node.start_position().row;
        let close_row = node.end_position().row;
        let has_ebd = all_body_children
            .iter()
            .any(|c| c.kind() == "enum_body_declarations");
        let has_comments = all_body_children.iter().any(|c| c.is_extra());
        let enum_constants: Vec<&Node> = all_body_children
            .iter()
            .filter(|c| c.kind() == "enum_constant")
            .collect();
        let all_constants_simple = !enum_constants.is_empty()
            && enum_constants.iter().all(|c| {
                c.child_by_field_name("body").is_none()
                    && c.start_position().row == c.end_position().row
            });
        open_row == close_row && !has_ebd && !has_comments && all_constants_simple
    } else {
        false
    }
}

#[derive(Default)]
pub(super) struct ClassBodyState {
    pub(super) end_row: Option<usize>,
    pub(super) kind: Option<String>,
    pub(super) structural_kind: Option<String>,
    pub(super) was_inline_comment: bool,
    pub(super) first_member_after_brace_comment: bool,
    pub(super) member_category: Option<MemberCategory>,
}

pub(super) struct ClassBodyLayout<'nodes, 'tree> {
    pub(super) node: Node<'tree>,
    pub(super) children: &'nodes [Node<'tree>],
    pub(super) simple_enum: bool,
    pub(super) blank_after_header: usize,
    pub(super) member_spacing: MemberSpacing,
}

impl<'a> BlockBuilder<'a> {
    pub(super) fn class_member_spacing(
        &self,
        layout: &ClassBodyLayout<'_, '_>,
        child_idx: usize,
        state: &mut ClassBodyState,
        source_inline_comment: bool,
    ) -> Spacing {
        let node = layout.node;
        let all_body_children = layout.children;
        let child = all_body_children[child_idx];
        let is_simple_enum_body = layout.simple_enum;
        let blank_after_header = layout.blank_after_header;
        let blank_lines_for = |category| layout.member_spacing.blank_lines(category);
        if let Some(prev_row) = state.end_row {
            let row_gap = child.start_position().row.saturating_sub(prev_row);
            if matches!(child.kind(), "enum_body_declarations" | ";")
                && node.kind() == "enum_body"
                && (state.kind.as_deref() == Some(",")
                    || state.structural_kind.as_deref() == Some(","))
            {
                let capped = row_gap.min(self.cfg.keep_blank_lines_in_declarations + 1);
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: capped.max(1) as u32,
                    keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                    keep_line_breaks: true,
                }
            } else if child.kind() == ";"
                && node.kind() != "enum_body"
                && state.kind.as_deref().is_some_and(|k| {
                    matches!(
                        k,
                        "method_declaration"
                            | "constructor_declaration"
                            | "class_declaration"
                            | "interface_declaration"
                            | "enum_declaration"
                            | "field_declaration"
                            | "constant_declaration"
                    )
                })
            {
                let blanks = state
                    .kind
                    .as_deref()
                    .and_then(MemberCategory::from_kind)
                    .map_or(0, blank_lines_for);
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: (1 + blanks) as u32,
                    keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                    keep_line_breaks: true,
                }
            } else if matches!(child.kind(), ";" | "enum_body_declarations")
                && node.kind() == "enum_body"
                && row_gap >= 2
            {
                let capped = row_gap.min(self.cfg.keep_blank_lines_in_declarations + 1);
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: capped as u32,
                    keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                    keep_line_breaks: true,
                }
            } else if child.kind() == ";"
                && node.kind() != "enum_body"
                && state
                    .kind
                    .as_deref()
                    .is_some_and(|k| MemberCategory::from_kind(k).is_some())
            {
                let pk = state.kind.as_deref().unwrap_or("");
                let blanks = if matches!(pk, "block" | "static_initializer") {
                    self.cfg.blank_lines_around_initializer
                } else {
                    MemberCategory::from_kind(pk).map_or(0, blank_lines_for)
                };
                let preserved = if row_gap >= 2 {
                    row_gap.min(self.cfg.keep_blank_lines_in_declarations + 1) as u32
                } else {
                    0
                };
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: ((1 + blanks) as u32).max(preserved),
                    keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                    keep_line_breaks: true,
                }
            } else if child.kind() == ","
                || child.kind() == ";"
                || child.kind() == "enum_body_declarations"
                || child.kind() == "record_declaration_body"
            {
                Spacing::none()
            } else if child.kind() == "}"
                && (state.kind.as_deref() == Some("{")
                    || state.structural_kind.as_deref() == Some("{"))
            {
                let effective_header = if node.kind() == "enum_body" {
                    0
                } else {
                    blank_after_header
                };
                let feeds = 1 + effective_header;
                let preserved = if row_gap >= 2 {
                    row_gap.min(self.cfg.keep_blank_lines_before_rbrace + 1) as u32
                } else {
                    0
                };
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: (feeds as u32).max(preserved),
                    keep_blank_lines: self.cfg.keep_blank_lines_before_rbrace as u32,
                    keep_line_breaks: true,
                }
            } else if child.kind() == "}" && is_simple_enum_body {
                Spacing::none()
            } else if child.kind() == "}" {
                let capped = row_gap
                    .min(self.cfg.keep_blank_lines_before_rbrace + 1)
                    .max(1);
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: capped as u32,
                    keep_blank_lines: self.cfg.keep_blank_lines_before_rbrace as u32,
                    keep_line_breaks: true,
                }
            } else if state.kind.as_deref() == Some("{") && is_simple_enum_body {
                Spacing::none()
            } else if state.kind.as_deref() == Some("{") && row_gap == 0 && child.is_extra() {
                state.first_member_after_brace_comment =
                    child.start_position().row == child.end_position().row;
                Spacing::one_space()
            } else if state.kind.as_deref() == Some("{") || state.first_member_after_brace_comment {
                state.first_member_after_brace_comment = false;
                let is_enum_constant = child.kind() == "enum_constant";
                let is_comment = child.is_extra();
                let is_enum_body = node.kind() == "enum_body";
                let effective_blank = if is_enum_constant || is_enum_body {
                    if row_gap >= 2 { 1 } else { 0 }
                } else if is_comment && row_gap < 2 {
                    let is_standalone = {
                        let mut last_end_row = child.end_position().row;
                        let mut result = true;
                        for &sib in &all_body_children[(child_idx + 1)..] {
                            if matches!(sib.kind(), "{" | "}" | "," | ";") {
                                break;
                            }
                            let gap = sib.start_position().row.saturating_sub(last_end_row);
                            if gap >= 2 {
                                break;
                            }
                            if sib.is_extra() {
                                let sib_text = std::str::from_utf8(
                                    &self.src[sib.start_byte()..sib.end_byte()],
                                )
                                .unwrap_or("");
                                let is_javadoc = sib.kind() == "block_comment"
                                    && sib_text.starts_with("/**")
                                    && !sib_text.starts_with("/***");
                                if is_javadoc {
                                    break;
                                }
                                last_end_row = sib.end_position().row;
                                continue;
                            }
                            result = false;
                            break;
                        }
                        result
                    };
                    if is_standalone { 0 } else { blank_after_header }
                } else {
                    blank_after_header
                };
                let feeds = 1 + effective_blank;
                let preserved = if row_gap >= 2 {
                    row_gap.min(self.cfg.keep_blank_lines_in_declarations + 1) as u32
                } else {
                    0
                };
                Spacing {
                    min_spaces: 0,
                    max_spaces: 0,
                    min_line_feeds: (feeds as u32).max(preserved),
                    keep_blank_lines: self.cfg.keep_blank_lines_in_declarations as u32,
                    keep_line_breaks: true,
                }
            } else if source_inline_comment && !self.is_doc_comment_node(child) {
                if child_idx > 0 {
                    let prev_sib = all_body_children[child_idx - 1];
                    let pb = prev_sib.end_byte();
                    let gap_end = child.start_byte().min(self.src.len());
                    if pb < gap_end {
                        let gap = &self.src[pb..gap_end];
                        let ts = self.cfg.indent_size.max(1) as u32;
                        let spaces = gap
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
                            .sum::<u32>();
                        Spacing::spaces(spaces, spaces)
                    } else {
                        Spacing::none()
                    }
                } else {
                    Spacing::one_space()
                }
            } else {
                let cur_cat = MemberCategory::from_kind(child.kind());
                let required_blanks =
                    self.required_member_blank_lines(layout, child_idx, state, row_gap);
                let members_share_source_line = row_gap == 0
                    && state
                        .kind
                        .as_deref()
                        .and_then(MemberCategory::from_kind)
                        .is_some()
                    && cur_cat.is_some();
                let min_feeds = if required_blanks > 0 {
                    (1 + required_blanks) as u32
                } else if row_gap >= 1 || members_share_source_line {
                    1
                } else {
                    0
                };
                let cap_keep = self.member_blank_line_limit(all_body_children, child_idx, row_gap);
                let capped = if row_gap >= 2 {
                    row_gap.min(cap_keep + 1) as u32
                } else {
                    0
                };
                let final_feeds = min_feeds.max(capped);
                if final_feeds == 0 {
                    Spacing::one_space()
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
        }
    }

    fn required_member_blank_lines(
        &self,
        layout: &ClassBodyLayout<'_, '_>,
        child_idx: usize,
        state: &ClassBodyState,
        row_gap: usize,
    ) -> usize {
        let node = layout.node;
        let all_body_children = layout.children;
        let child = all_body_children[child_idx];
        let cur_cat = MemberCategory::from_kind(child.kind());
        let blank_lines_for = |category| layout.member_spacing.blank_lines(category);
        let prev_is_comment = state
            .kind
            .as_deref()
            .is_some_and(|k| k == "line_comment" || k == "block_comment")
            && !state.was_inline_comment;
        let standalone_before_doc = child.is_extra()
            && all_body_children[(child_idx + 1)..]
                .iter()
                .take_while(|c| c.is_extra())
                .any(|c| self.is_doc_comment_node(*c));
        let inline_comment_after_doc = state.was_inline_comment
            && child_idx >= 2
            && self.is_doc_comment_node(all_body_children[child_idx - 2]);
        if standalone_before_doc || inline_comment_after_doc {
            0
        } else if prev_is_comment {
            let is_enum_body = node.kind() == "enum_body";
            let is_javadoc_here = child.kind() == "block_comment"
                && self.text(child).starts_with("/**")
                && !self.text(child).starts_with("/***");
            let next_member_is_method_like = if is_javadoc_here {
                all_body_children[(child_idx + 1)..]
                    .iter()
                    .find(|c| !c.is_extra())
                    .and_then(|c| MemberCategory::from_kind(c.kind()))
                    .is_some_and(|cat| {
                        matches!(cat, MemberCategory::Method | MemberCategory::Class)
                    })
            } else {
                false
            };
            if !is_enum_body
                && state.kind.as_deref() == Some("line_comment")
                && is_javadoc_here
                && next_member_is_method_like
            {
                1
            } else {
                0
            }
        } else {
            match (state.member_category, cur_cat) {
                (Some(prev), Some(cur)) => blank_lines_for(prev).max(blank_lines_for(cur)),
                (Some(prev), None) if child.is_extra() => {
                    let is_javadoc = self.is_doc_comment_node(child);
                    let (next_member_blank, leads_member) = if !is_javadoc && row_gap < 2 {
                        let mut last_row = child.end_position().row;
                        let mut found = 0usize;
                        let mut found_member = false;
                        for &sib in &all_body_children[(child_idx + 1)..] {
                            let gap = sib.start_position().row.saturating_sub(last_row);
                            if gap >= 2 {
                                break;
                            }
                            if sib.is_extra() {
                                let sib_text = std::str::from_utf8(
                                    &self.src[sib.start_byte()..sib.end_byte()],
                                )
                                .unwrap_or("");
                                let sib_is_javadoc = sib.kind() == "block_comment"
                                    && sib_text.starts_with("/**")
                                    && !sib_text.starts_with("/***");
                                if sib_is_javadoc {
                                    break;
                                }
                                last_row = sib.end_position().row;
                                continue;
                            }
                            if let Some(cat) = MemberCategory::from_kind(sib.kind()) {
                                found = blank_lines_for(cat);
                                found_member = true;
                            }
                            break;
                        }
                        (found, found_member)
                    } else if is_javadoc {
                        let blank = all_body_children[(child_idx + 1)..]
                            .iter()
                            .find(|c| !c.is_extra())
                            .and_then(|c| MemberCategory::from_kind(c.kind()))
                            .map_or(0, blank_lines_for);
                        (blank, true)
                    } else {
                        (0, false)
                    };
                    if row_gap >= 2 || is_javadoc || leads_member {
                        blank_lines_for(prev).max(next_member_blank)
                    } else {
                        0
                    }
                }
                _ => 0,
            }
        }
    }

    fn member_blank_line_limit(
        &self,
        all_body_children: &[Node],
        child_idx: usize,
        row_gap: usize,
    ) -> usize {
        let child = all_body_children[child_idx];
        if child.is_extra() && row_gap >= 2 {
            let is_javadoc_child = child.kind() == "block_comment" && {
                let t = self.text(child);
                t.starts_with("/**") && !t.starts_with("/***")
            };
            let is_non_javadoc_comment = !is_javadoc_child
                && (child.kind() == "line_comment" || child.kind() == "block_comment");
            let next_gap_blank = if is_non_javadoc_comment {
                let mut last_end_row = child.end_position().row;
                let mut idx = child_idx + 1;
                while let Some(&sib) = all_body_children.get(idx) {
                    let gap = sib.start_position().row.saturating_sub(last_end_row);
                    if gap >= 2 {
                        break;
                    }
                    if !sib.is_extra() {
                        break;
                    }
                    let sib_text = std::str::from_utf8(&self.src[sib.start_byte()..sib.end_byte()])
                        .unwrap_or("");
                    let sib_is_javadoc = sib.kind() == "block_comment"
                        && sib_text.starts_with("/**")
                        && !sib_text.starts_with("/***");
                    if sib_is_javadoc {
                        break;
                    }
                    last_end_row = sib.end_position().row;
                    idx += 1;
                }
                all_body_children
                    .get(idx)
                    .is_none_or(|sib| sib.start_position().row.saturating_sub(last_end_row) >= 2)
            } else {
                false
            };
            if is_non_javadoc_comment && child.kind() == "line_comment" && next_gap_blank {
                self.cfg.keep_blank_lines_in_code
            } else {
                self.cfg.keep_blank_lines_in_declarations
            }
        } else {
            self.cfg.keep_blank_lines_in_declarations
        }
    }
}
