use std::collections::HashMap;
use tree_sitter::Node;

use crate::config::WrapMode;
use crate::formatter::build::BlockBuilder;
use crate::formatter::build::util::wrap_type_from_mode;
use crate::formatter::ir::*;

type DeclarationPadding = (usize, usize, usize);
type DeclarationAlignments = HashMap<usize, DeclarationPadding>;
type ContinuationAlignments = HashMap<usize, usize>;

fn eq_wrapped_declarator_row(decl: Node, head_row: Option<usize>) -> bool {
    let mut vc = decl.walk();
    let eq = decl.children(&mut vc).find(|c| c.kind() == "=");
    match (eq, head_row) {
        (Some(eq), Some(hr)) => eq.start_position().row > hr,
        _ => false,
    }
}

fn eq_wrapped_declarator(node: Node) -> Option<usize> {
    let mut c = node.walk();
    let kids: Vec<Node> = node.children(&mut c).collect();
    let mut prev_row: Option<usize> = None;
    for k in kids.iter() {
        if k.kind() == "variable_declarator" {
            let opens_row = prev_row.is_some_and(|r| k.start_position().row > r);
            if k.child_by_field_name("value").is_some() {
                if opens_row {
                    return None;
                }
                let mut head_row = k.child_by_field_name("name").map(|n| n.end_position().row);
                let mut vc = k.walk();
                for d in k.children(&mut vc) {
                    if d.kind() == "dimensions" {
                        head_row = Some(d.end_position().row);
                    }
                }
                return if eq_wrapped_declarator_row(*k, head_row) {
                    Some(k.id())
                } else {
                    None
                };
            }
        }
        if k.kind() != ";" {
            prev_row = Some(k.end_position().row);
        }
    }
    None
}

impl<'a> BlockBuilder<'a> {
    fn field_parts_for_align(
        &self,
        node: Node,
        is_local_var: bool,
    ) -> (Vec<String>, String, String, String, bool) {
        let mut kw_mods: Vec<String> = Vec::new();
        let mut type_text = String::new();
        let mut name_text = String::new();
        let mut has_value = false;
        let mut type_use_prefix = String::new();

        let type_row: Option<usize> = {
            let mut tc = node.walk();
            node.children(&mut tc)
                .find(|c| c.kind() != "modifiers" && !c.is_extra())
                .map(|c| c.start_position().row)
        };

        let mut cursor = node.walk();
        let mut decl_prev_row: Option<usize> = None;
        let mut names_closed = false;
        for child in node.children(&mut cursor) {
            if child.kind() != ";" && child.kind() != "variable_declarator" {
                decl_prev_row = Some(child.end_position().row);
            }
            match child.kind() {
                "modifiers" => {
                    let mut mc = child.walk();
                    let mod_children: Vec<Node> = child.children(&mut mc).collect();
                    let mut seen_keyword = false;
                    let field_ann_no_wrap =
                        matches!(self.cfg.field_annotation_wrap, WrapMode::DoNotWrap);
                    for (idx, m) in mod_children.iter().enumerate() {
                        match m.kind() {
                            "marker_annotation" | "annotation" => {
                                let next_row = mod_children[(idx + 1)..]
                                    .iter()
                                    .find(|ns| !ns.is_extra())
                                    .map(|ns| ns.start_position().row)
                                    .or(type_row);
                                let source_inline = next_row
                                    .map(|nr| nr == m.end_position().row)
                                    .unwrap_or(false);
                                let first_kw_row = mod_children
                                    .iter()
                                    .find(|c| {
                                        !matches!(c.kind(), "marker_annotation" | "annotation")
                                            && !c.is_extra()
                                    })
                                    .map(|c| c.start_position().row);
                                let decl_row = first_kw_row.or(type_row);
                                let on_same_line_as_decl = decl_row
                                    .map(|dr| m.end_position().row == dr)
                                    .unwrap_or(false);
                                let is_last_modifier =
                                    mod_children[(idx + 1)..].iter().all(|ns| ns.is_extra());
                                let is_inline = if field_ann_no_wrap || is_local_var {
                                    on_same_line_as_decl
                                } else if seen_keyword {
                                    source_inline
                                } else {
                                    source_inline
                                        && is_last_modifier
                                        && m.kind() == "marker_annotation"
                                        && self.is_type_use_annotation(*m)
                                };
                                if is_inline {
                                    let ann_text = self.text(*m).trim().to_owned();
                                    let go_to_type = is_local_var;
                                    if go_to_type {
                                        if !type_use_prefix.is_empty() {
                                            type_use_prefix.push(' ');
                                        }
                                        type_use_prefix.push_str(&ann_text);
                                    } else {
                                        kw_mods.push(ann_text);
                                    }
                                }
                            }
                            _ if m.is_extra() => {}
                            _ => {
                                if !is_local_var
                                    && wrap_type_from_mode(self.cfg.field_annotation_wrap)
                                        == WrapType::Always
                                    && idx > 0
                                    && matches!(
                                        mod_children[idx - 1].kind(),
                                        "marker_annotation" | "annotation"
                                    )
                                {
                                    kw_mods.clear();
                                }
                                let t = self.text(*m).trim().to_owned();
                                if !t.is_empty() {
                                    kw_mods.push(t);
                                    seen_keyword = true;
                                }
                            }
                        }
                    }
                }
                ";" => {}
                "variable_declarator" => {
                    if decl_prev_row.is_some_and(|r| child.start_position().row > r) {
                        names_closed = true;
                    }
                    if !has_value && !names_closed {
                        let name_node = child.child_by_field_name("name");
                        if !name_text.is_empty() {
                            name_text.push_str(", ");
                        }
                        if let Some(n) = name_node {
                            name_text.push_str(self.text(n).trim());
                        }
                        let mut head_row = name_node.map(|n| n.end_position().row);
                        {
                            let mut vc = child.walk();
                            for dim_child in child.children(&mut vc) {
                                if dim_child.kind() == "dimensions" {
                                    name_text.push_str(self.text(dim_child));
                                    head_row = Some(dim_child.end_position().row);
                                }
                            }
                        }
                        if child.child_by_field_name("value").is_some() {
                            if eq_wrapped_declarator_row(child, head_row) {
                                names_closed = true;
                            } else {
                                has_value = true;
                            }
                        }
                    }
                    decl_prev_row = Some(child.end_position().row);
                }
                _ if child.is_extra()
                    && child.kind() == "block_comment"
                    && type_text.is_empty()
                    && !kw_mods.is_empty()
                    && child.start_position().row == child.end_position().row
                    && type_row == Some(child.end_position().row) =>
                {
                    let t = self.text(child).trim().to_owned();
                    if !t.is_empty() {
                        kw_mods.push(t);
                    }
                }
                _ if child.kind().ends_with("_type")
                    || child.kind() == "type_identifier"
                    || child.kind() == "generic_type"
                    || child.kind() == "array_type"
                    || child.kind() == "integral_type"
                    || child.kind() == "floating_point_type"
                    || child.kind() == "boolean_type"
                    || child.kind() == "void_type"
                    || child.kind() == "annotated_type"
                    || child.kind() == "scoped_type_identifier" =>
                {
                    type_text = normalize_type_for_align(self.text(child).trim());
                }
                _ => {}
            }
        }

        if !type_use_prefix.is_empty() && !type_text.is_empty() {
            type_text = format!("{} {}", type_use_prefix, type_text);
        }

        let kw_str = kw_mods.join(" ");
        (vec![], kw_str, type_text, name_text, has_value)
    }

    pub(crate) fn compute_local_var_group_alignment(
        &self,
        members: &[Node],
    ) -> (DeclarationAlignments, ContinuationAlignments) {
        let is_local_var = |n: &Node| {
            if n.kind() != "local_variable_declaration" {
                return false;
            }
            n.start_position().row == n.end_position().row
        };
        self.compute_decl_group_alignment(members, is_local_var, true, false, true)
    }

    pub(in crate::formatter::build) fn compute_field_group_alignment(
        &self,
        members: &[Node],
    ) -> (DeclarationAlignments, ContinuationAlignments) {
        let is_field = |n: &Node| matches!(n.kind(), "field_declaration" | "constant_declaration");
        self.compute_decl_group_alignment(members, is_field, false, true, false)
    }

    fn compute_decl_group_alignment(
        &self,
        members: &[Node],
        is_decl: impl Fn(&Node) -> bool,
        comment_breaks_group: bool,
        split_subgroups_by_modifier: bool,
        is_local_var: bool,
    ) -> (DeclarationAlignments, ContinuationAlignments) {
        let mut result = DeclarationAlignments::new();
        let mut cont = ContinuationAlignments::new();

        let has_multiple_declarators = |n: &Node| -> bool {
            let mut cursor = n.walk();
            n.children(&mut cursor)
                .filter(|c| c.kind() == "variable_declarator")
                .count()
                > 1
        };

        let mut i = 0;
        while i < members.len() {
            if !is_decl(&members[i]) {
                i += 1;
                continue;
            }
            let start = i;
            let mut prev_end_row = members[i].end_position().row;
            i += 1;
            while i < members.len() {
                let gap = members[i]
                    .start_position()
                    .row
                    .saturating_sub(prev_end_row + 1);
                if members[i].is_extra() {
                    if gap > 0 {
                        break;
                    }
                    if comment_breaks_group {
                        let comment_start_row = members[i].start_position().row;
                        let comment_is_own_line = comment_start_row > prev_end_row;
                        if comment_is_own_line {
                            break;
                        }
                    }
                    prev_end_row = members[i].end_position().row;
                    i += 1;
                    continue;
                }
                if !is_decl(&members[i]) {
                    break;
                }
                if members[i].start_position().row <= prev_end_row
                    && members.get(i + 1).is_some_and(|next| {
                        next.is_extra()
                            && next.kind() == "line_comment"
                            && next.start_position().row == members[i].end_position().row
                    })
                {
                    break;
                }
                if gap > 0 {
                    break;
                }
                prev_end_row = members[i].end_position().row;
                i += 1;
            }

            let group: Vec<Node> = members[start..i]
                .iter()
                .filter(|n| is_decl(n))
                .copied()
                .collect();

            let header_multiline = |n: &Node| -> bool {
                let mut c = n.walk();
                let mut prev_end: Option<usize> = None;
                for ch in n.children(&mut c) {
                    if let Some(p) = prev_end
                        && ch.start_position().row > p
                    {
                        return true;
                    }
                    if ch.kind() == "variable_declarator" {
                        return false;
                    }
                    prev_end = Some(ch.end_position().row);
                }
                false
            };
            let mut j = 0;
            while j < group.len() {
                let sg_start = j;
                let first_has_mods = {
                    let mut c = group[j].walk();
                    group[j].children(&mut c).any(|ch| ch.kind() == "modifiers")
                };
                let first_multiline = header_multiline(&group[j]);
                while j < group.len() {
                    let is_multi = has_multiple_declarators(&group[j]);
                    let cur_has_mods = {
                        let mut c = group[j].walk();
                        group[j].children(&mut c).any(|ch| ch.kind() == "modifiers")
                    };
                    if split_subgroups_by_modifier
                        && j > sg_start
                        && (cur_has_mods != first_has_mods
                            || header_multiline(&group[j]) != first_multiline)
                    {
                        break;
                    }
                    j += 1;
                    if is_multi && !is_local_var {
                        break;
                    }
                }
                let subgroup = &group[sg_start..j];

                let parts: Vec<(Vec<String>, String, String, String, bool)> = subgroup
                    .iter()
                    .map(|n| self.field_parts_for_align(*n, is_local_var))
                    .collect();

                let max_mod = parts
                    .iter()
                    .map(|(_, km, _, _, _)| km.len())
                    .max()
                    .unwrap_or(0);
                let max_type = parts
                    .iter()
                    .map(|(_, _, t, _, _)| t.len())
                    .max()
                    .unwrap_or(0);
                let first_wrapped: Vec<bool> = subgroup
                    .iter()
                    .map(|n| {
                        let mut c = n.walk();
                        let mut prev_row: Option<usize> = None;
                        for k in n.children(&mut c) {
                            if k.kind() == "variable_declarator" {
                                return prev_row.is_some_and(|r| k.start_position().row > r);
                            }
                            if k.kind() != ";" {
                                prev_row = Some(k.end_position().row);
                            }
                        }
                        false
                    })
                    .collect();
                let max_name = parts
                    .iter()
                    .zip(first_wrapped.iter())
                    .filter(|((_, _, _, _, hv), fw)| *hv && !**fw)
                    .map(|((_, _, _, n, _), _)| n.len())
                    .max()
                    .unwrap_or(0);

                let any_has_no_mods = parts.iter().any(|(_, km, _, _, _)| km.is_empty());
                let any_has_mods = parts.iter().any(|(_, km, _, _, _)| !km.is_empty());
                let mixed_mods = any_has_mods && any_has_no_mods;
                let max_effective_col = if mixed_mods {
                    parts
                        .iter()
                        .map(|(_, km, t, _, _)| {
                            if km.is_empty() {
                                t.len()
                            } else {
                                km.len() + 1 + t.len()
                            }
                        })
                        .max()
                        .unwrap_or(0)
                } else {
                    0
                };

                let name_col_rel = if mixed_mods {
                    max_effective_col + 1
                } else if any_has_mods {
                    max_mod + 1 + max_type + 1
                } else {
                    max_type + 1
                };
                let cont_indent = self.cfg.continuation_indent;
                let mut cont_decls: Vec<(usize, usize)> = Vec::new();
                for n in subgroup.iter() {
                    let mut c = n.walk();
                    let kids: Vec<Node> = n.children(&mut c).collect();
                    let mut prev_row: Option<usize> = None;
                    let mut row_prefix = 0usize;
                    let mut on_cont_row = false;
                    let mut row_done = false;
                    for k in kids.iter() {
                        if k.kind() == "variable_declarator" {
                            if prev_row.is_some_and(|r| k.start_position().row > r) {
                                row_prefix = 0;
                                on_cont_row = true;
                                row_done = false;
                            }
                            if on_cont_row && !row_done {
                                if k.child_by_field_name("value").is_some() {
                                    let mut head_end = k
                                        .child_by_field_name("name")
                                        .map(|x| x.end_byte())
                                        .unwrap_or(k.start_byte());
                                    let mut vc = k.walk();
                                    for d in k.children(&mut vc) {
                                        if d.kind() == "dimensions" {
                                            head_end = head_end.max(d.end_byte());
                                        }
                                    }
                                    cont_decls.push((
                                        k.id(),
                                        row_prefix + head_end.saturating_sub(k.start_byte()),
                                    ));
                                    row_done = true;
                                } else {
                                    row_prefix += (k.end_byte() - k.start_byte()) + 2;
                                }
                            }
                        }
                        if k.kind() != ";" {
                            prev_row = Some(k.end_position().row);
                        }
                    }
                }
                let eq_col_rel = if max_name > 0 {
                    cont_decls
                        .iter()
                        .map(|(_, l)| cont_indent + l + 1)
                        .chain(std::iter::once(name_col_rel + max_name + 1))
                        .max()
                        .unwrap_or(0)
                } else {
                    cont_decls
                        .iter()
                        .map(|(_, l)| cont_indent + l + 1)
                        .max()
                        .unwrap_or(0)
                };
                for (id, l) in cont_decls.iter() {
                    let pad = eq_col_rel.saturating_sub(cont_indent + l + 1);
                    if pad > 0 {
                        cont.insert(*id, pad);
                    }
                }
                if eq_col_rel > 0 {
                    for n in subgroup.iter() {
                        if let Some(id) = eq_wrapped_declarator(*n) {
                            cont.insert(id, eq_col_rel);
                        }
                    }
                }

                let unmodified_primitive_and_generic_array = is_local_var
                    && subgroup.len() == 2
                    && parts
                        .iter()
                        .all(|(_, modifiers, _, _, _)| modifiers.is_empty())
                    && parts[0].3.len() == parts[1].3.len()
                    && subgroup.iter().any(|declaration| {
                        declaration.child_by_field_name("type").is_some_and(|ty| {
                            matches!(
                                ty.kind(),
                                "integral_type" | "floating_point_type" | "boolean_type"
                            )
                        })
                    })
                    && subgroup.iter().any(|declaration| {
                        declaration.child_by_field_name("type").is_some_and(|ty| {
                            ty.kind() == "array_type"
                                && ty
                                    .named_child(0)
                                    .is_some_and(|element| element.kind() == "generic_type")
                        })
                    });

                for (field_node, (_, km, type_text, name, hv)) in subgroup.iter().zip(parts.iter())
                {
                    let has_mods = !km.is_empty();
                    let mod_pad = if has_mods && km.len() < max_mod {
                        max_mod - km.len()
                    } else {
                        0
                    };
                    let type_len = type_text.len();
                    let type_pad = if unmodified_primitive_and_generic_array {
                        0
                    } else if mixed_mods && !has_mods {
                        max_effective_col.saturating_sub(type_len)
                    } else if mixed_mods && has_mods {
                        max_effective_col.saturating_sub(max_mod + type_len + 1)
                    } else {
                        max_type.saturating_sub(type_len)
                    };
                    let name_pad = if *hv {
                        if unmodified_primitive_and_generic_array {
                            eq_col_rel.saturating_sub(type_len + name.len() + 2)
                        } else {
                            eq_col_rel.saturating_sub(name_col_rel + name.len() + 1)
                        }
                    } else {
                        0
                    };
                    result.insert(field_node.id(), (mod_pad, type_pad, name_pad));
                }
            }
        }
        (result, cont)
    }

    pub(crate) fn field_declaration_aligned(
        &mut self,
        node: Node,
        mod_pad: usize,
        type_pad: usize,
        name_pad: usize,
        cont_pads: &HashMap<usize, usize>,
    ) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let all_children: Vec<Node> = node.children(&mut cursor).collect();

        let mut prev_end_byte: Option<usize> = None;
        let mut after_modifiers = false;
        let mut after_type = false;
        let mut declarator_idx = 0;
        let mut pad_placed = false;
        let eq_wrap_id = eq_wrapped_declarator(node);

        for &child in all_children.iter() {
            let kind = child.kind();

            if kind == "modifiers" {
                let mod_block = self.modifiers_and_annotations(child);
                children.push(mod_block);
                if mod_pad > 0 {
                    let pad = " ".repeat(mod_pad);
                    children.push(Block::leaf(pad));
                }
                after_modifiers = true;
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            if kind == ";" {
                children.push(Block::leaf(";").with_spacing(Spacing::none()));
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            let is_type = kind.ends_with("_type")
                || kind == "type_identifier"
                || kind == "generic_type"
                || kind == "array_type"
                || kind == "integral_type"
                || kind == "floating_point_type"
                || kind == "boolean_type"
                || kind == "void_type"
                || kind == "annotated_type"
                || kind == "scoped_type_identifier";

            if is_type && !after_type {
                let type_block = self.node(child);
                let spacing = if after_modifiers {
                    let mod_node = all_children.iter().find(|c| c.kind() == "modifiers");
                    let mod_end_row = mod_node.map(|c| c.end_position().row).unwrap_or(0);
                    let has_decl_annotations = mod_node.is_some_and(|mn| {
                        let mut mc = mn.walk();
                        let kids: Vec<_> = mn.children(&mut mc).filter(|c| !c.is_extra()).collect();
                        let last = kids.last();
                        match last {
                            Some(last)
                                if matches!(last.kind(), "marker_annotation" | "annotation") =>
                            {
                                let has_keyword_before =
                                    kids.iter().take(kids.len() - 1).any(|c| {
                                        !matches!(c.kind(), "marker_annotation" | "annotation")
                                            && !c.is_extra()
                                            && c.child_count() == 0
                                    });
                                if has_keyword_before {
                                    false
                                } else if last.kind() == "marker_annotation" {
                                    if node.kind() == "local_variable_declaration" {
                                        !self.is_type_use_annotation_for_align(*last)
                                    } else {
                                        !self.is_type_use_annotation(*last)
                                    }
                                } else {
                                    true
                                }
                            }
                            _ => false,
                        }
                    });
                    let anno_wrap = if node.kind() == "local_variable_declaration" {
                        WrapMode::DoNotWrap
                    } else {
                        self.cfg.field_annotation_wrap
                    };
                    if has_decl_annotations && anno_wrap != WrapMode::DoNotWrap {
                        Spacing::new_line()
                    } else if child.start_position().row > mod_end_row {
                        let gap = child.start_position().row - mod_end_row;
                        if node.kind() == "local_variable_declaration" && gap > 1 {
                            let capped = gap.min(self.cfg.keep_blank_lines_in_code + 1);
                            Spacing {
                                min_spaces: 0,
                                max_spaces: 0,
                                min_line_feeds: capped as u32,
                                keep_blank_lines: self.cfg.keep_blank_lines_in_code as u32,
                                keep_line_breaks: true,
                            }
                        } else {
                            Spacing::new_line()
                        }
                    } else {
                        Spacing::one_space()
                    }
                } else if prev_end_byte.is_some() {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                };
                let type_block = if prev_end_byte.is_some() {
                    type_block.with_spacing(spacing)
                } else {
                    type_block
                };
                children.push(type_block);
                if type_pad > 0 {
                    let pad = " ".repeat(type_pad);
                    children.push(Block::leaf(pad));
                }
                after_type = true;
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            if kind == "variable_declarator" {
                let eq_wrap_col = if eq_wrap_id == Some(child.id()) {
                    cont_pads.get(&child.id()).copied()
                } else {
                    None
                };
                let cont_pad = if eq_wrap_col.is_some() {
                    None
                } else {
                    cont_pads.get(&child.id()).copied()
                };
                let is_pad_target = cont_pad.is_none()
                    && eq_wrap_col.is_none()
                    && !pad_placed
                    && child.child_by_field_name("value").is_some();
                if is_pad_target || eq_wrap_col.is_some() {
                    pad_placed = true;
                }
                let pad = cont_pad.unwrap_or(if is_pad_target { name_pad } else { 0 });
                let decl_block = self.variable_declarator_aligned(child, pad, eq_wrap_col);
                let on_new_line = {
                    let prev_row = all_children
                        .iter()
                        .rfind(|c| c.end_byte() <= child.start_byte() && c.kind() != ";")
                        .map(|c| c.end_position().row);
                    prev_row.is_some_and(|pr| child.start_position().row > pr)
                        && (declarator_idx > 0 || (type_pad == 0 && pad == 0))
                };
                let spacing = if on_new_line {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                };
                let decl_block = if on_new_line {
                    decl_block.with_indent(IndentType::Continuation)
                } else {
                    decl_block
                };
                let decl_block = if prev_end_byte.is_some() {
                    decl_block.with_spacing(spacing)
                } else {
                    decl_block
                };
                children.push(decl_block);
                declarator_idx += 1;
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            if kind == "," {
                children.push(Block::leaf(",").with_spacing(Spacing::none()));
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            let block = if child.is_extra() {
                self.comment_leaf(child)
            } else {
                self.node(child)
            };
            let spacing = if let Some(pb) = prev_end_byte {
                let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                if gap.contains(&b'\n') {
                    Spacing::new_line()
                } else if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                }
            } else {
                Spacing::none()
            };
            let block = if prev_end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };
            children.push(block);
            prev_end_byte = Some(child.end_byte());
        }

        Block::node(children)
    }

    fn variable_declarator_aligned(
        &mut self,
        node: Node,
        name_pad: usize,
        eq_wrap_col: Option<usize>,
    ) -> Block {
        if name_pad == 0 && eq_wrap_col.is_none() {
            return self.variable_declarator(node);
        }
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let kids: Vec<Node> = node.children(&mut cursor).collect();
        let mut prev_end_byte: Option<usize> = None;
        let mut name_emitted = false;
        let mut saw_equals = false;
        let mut prev_is_eq = false;

        for (kid_idx, child) in kids.iter().copied().enumerate() {
            let kind = child.kind();
            if kind == "=" {
                saw_equals = true;
                if let Some(col) = eq_wrap_col {
                    let sb = prev_end_byte.is_some_and(|pb| {
                        self.src[pb..child.start_byte().min(self.src.len())].contains(&b'\n')
                    });
                    if sb {
                        children
                            .push(Block::leaf(" ".repeat(col)).with_spacing(Spacing::new_line()));
                        children.push(Block::leaf("=").with_spacing(Spacing::none()));
                        prev_end_byte = Some(child.end_byte());
                        prev_is_eq = true;
                        continue;
                    }
                }
            }
            let block = if child.is_extra() {
                self.comment_leaf(child)
            } else {
                self.node(child)
            };
            let source_break = if let Some(pb) = prev_end_byte {
                let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                gap.contains(&b'\n')
            } else {
                false
            };
            let spacing = if prev_end_byte.is_none() {
                Spacing::none()
            } else if source_break {
                Spacing::new_line()
            } else if kind == "=" || prev_is_eq {
                Spacing::one_space()
            } else if let Some(pb) = prev_end_byte {
                let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                    Spacing::one_space()
                } else {
                    Spacing::none()
                }
            } else {
                Spacing::none()
            };
            let indent = if saw_equals && source_break {
                IndentType::Continuation
            } else {
                IndentType::None
            };
            let block = if prev_end_byte.is_some() {
                let block = block.with_spacing(spacing);
                if indent != IndentType::None {
                    block.with_indent(indent)
                } else {
                    block
                }
            } else {
                block
            };
            children.push(block);

            if !name_emitted && (kind == "identifier" || kind == "dimensions") {
                let dims_follow = kids[kid_idx + 1..]
                    .iter()
                    .find(|n| !n.is_extra())
                    .map(|n| n.kind() == "dimensions")
                    .unwrap_or(false);
                if !dims_follow {
                    name_emitted = true;
                    if name_pad > 0 {
                        let pad = " ".repeat(name_pad);
                        children.push(Block::leaf(pad));
                    }
                }
            }

            prev_end_byte = Some(child.end_byte());
            prev_is_eq = kind == "=";
        }

        Block::node(children)
    }
}

fn normalize_type_for_align(text: &str) -> String {
    fn is_word(c: char) -> bool {
        c.is_alphanumeric() || c == '_' || c == '$'
    }
    let mut out = String::with_capacity(text.len());
    let mut prev: Option<char> = None;
    let mut pending_ws = false;
    let mut prev_ann = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if prev.is_some() {
                pending_ws = true;
            }
            continue;
        }
        let sep = match prev {
            None => false,
            Some(_) if c == ',' => false,
            Some(p) => {
                if p == ',' || c == '&' || p == '&' || c == '=' || p == '=' {
                    true
                } else if pending_ws {
                    ((is_word(p) || p == '>' || p == '?' || p == ']' || p == ')')
                        && (is_word(c) || c == '@' || c == '?'))
                        || (prev_ann && c == '[')
                } else {
                    false
                }
            }
        };
        if sep {
            out.push(' ');
        }
        out.push(c);
        prev_ann = if c == '@' {
            true
        } else if is_word(c) || c == '(' || c == ')' {
            prev_ann
        } else {
            false
        };
        prev = Some(c);
        pending_ws = false;
    }
    out
}
