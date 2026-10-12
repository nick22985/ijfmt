use tree_sitter::Node;

use crate::formatter::build::BlockBuilder;
use crate::formatter::build::comments;
use crate::formatter::ir::*;

impl<'a> BlockBuilder<'a> {
    pub(crate) fn is_doc_comment_node(&self, node: Node) -> bool {
        is_doc_comment_token(self.src, node)
    }

    pub(crate) fn comment_leaf(&self, node: Node) -> Block {
        let w = self.comment_pre_run_width(node);
        self.comment_leaf_inner(node).with_pre_run_width(w)
    }

    fn comment_pre_run_width(&self, node: Node) -> usize {
        let mut b = node.start_byte();
        while b > 0 && matches!(self.src[b - 1], b' ' | b'\t') {
            b -= 1;
        }
        if b == 0 || b == node.start_byte() || matches!(self.src[b - 1], b'\n' | b'\r') {
            return 0;
        }
        let ts = self.cfg.indent_size.max(1);
        let mut col = 0usize;
        for &ch in &self.src[b..node.start_byte()] {
            col += if ch == b'\t' { ts - col % ts } else { 1 };
        }
        col
    }

    pub(in crate::formatter::build) fn md_run_reflowed(
        &self,
        node: Node,
    ) -> Option<(String, usize)> {
        if !self.cfg.wrap_comments || self.cfg.right_margin == 0 {
            return None;
        }
        if node.kind() != "line_comment" || !self.text(node).starts_with("///") {
            return None;
        }
        if node.parent().is_none_or(|p| {
            !matches!(
                p.kind(),
                "program"
                    | "class_body"
                    | "interface_body"
                    | "enum_body"
                    | "annotation_type_body"
                    | "record_declaration_body"
            )
        }) {
            return None;
        }
        let mut start = node;
        while let Some(p) = start.prev_sibling() {
            if md_run_linked(self.src, p, start) {
                start = p;
            } else {
                break;
            }
        }
        let mut run = vec![start];
        while let Some(n) = run[run.len() - 1].next_sibling() {
            if md_run_linked(self.src, run[run.len() - 1], n) {
                run.push(n);
            } else {
                break;
            }
        }
        if run.len() < 2 || !is_owned_markdown_doc(self.src, start) {
            return None;
        }
        if self
            .javadoc_stop_byte
            .is_some_and(|stop| start.end_byte() > stop)
        {
            return None;
        }
        let idx = run.iter().position(|n| n.id() == node.id())?;
        let indent = self.compute_output_col(start);
        let text = self.format_markdown_doc_run(&run, indent)?;
        let per_line = run
            .iter()
            .map(|n| format_markdown_doc_line(self.text(*n)))
            .collect::<Vec<_>>()
            .join("\n");
        if text == per_line {
            return None;
        }
        Some((text, idx))
    }

    fn format_markdown_doc_run(&self, run: &[Node], indent: usize) -> Option<String> {
        let margin = self.cfg.right_margin;
        let mut toks: Vec<(usize, &str)> = Vec::new();
        for n in run {
            let mut rest = markdown_doc_content(self.text(*n).trim())?;
            let mut first = true;
            while !rest.trim_start().is_empty() {
                let ws = rest.len() - rest.trim_start().len();
                rest = &rest[ws..];
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                toks.push((ws + usize::from(first), &rest[..end]));
                rest = &rest[end..];
                first = false;
            }
        }
        let mut out = String::from("///");
        let mut col = indent + 3;
        for (k, (sep, word)) in toks.iter().enumerate() {
            let w = word.chars().count();
            if k > 0 && col + sep + w > margin {
                out.push_str("\n/// ");
                col = indent + 4;
            } else {
                out.push_str(&" ".repeat(*sep));
                col += sep;
            }
            out.push_str(word);
            col += w;
        }
        Some(out)
    }

    fn comment_leaf_inner(&self, node: Node) -> Block {
        let raw = self.text(node);
        if node.kind() == "block_comment"
            && raw.starts_with("/**")
            && !raw.starts_with("/***")
            && is_doc_comment_position(node)
            && !is_orphan_doc_comment(self.src, node)
            && self
                .javadoc_stop_byte
                .is_none_or(|stop| node.end_byte() <= stop)
        {
            let output_col = self.compute_output_col(node);
            let processed = self.process_javadoc_for(raw, output_col, javadoc_owner_kind(node));
            Block::leaf(processed)
                .with_source_offset(node.start_byte())
                .with_jd_regenerated()
        } else if node.kind() == "line_comment"
            && raw.starts_with("///")
            && is_owned_markdown_doc(self.src, node)
            && self
                .javadoc_stop_byte
                .is_none_or(|stop| node.end_byte() <= stop)
        {
            let text = match self.md_run_reflowed(node) {
                Some((t, 0)) => t,
                _ => format_markdown_doc_line(raw),
            };
            Block::leaf(text).with_source_offset(node.start_byte())
        } else if node.kind() == "line_comment" {
            let trailing_ws = {
                let mut n = raw.len() - raw.trim_end().len();
                let mut p = node.end_byte();
                while p < self.src.len() && (self.src[p] == b' ' || self.src[p] == b'\t') {
                    n += 1;
                    p += 1;
                }
                n
            };
            let text = if raw.starts_with("///") {
                glue_markdown_comparison(raw.trim_end())
            } else {
                raw.trim_end().to_string()
            };
            Block::leaf(text)
                .with_source_offset(node.start_byte())
                .with_extra_width(trailing_ws)
                .with_line_comment()
        } else if node.kind() == "block_comment" && raw.starts_with("/**") {
            Block::leaf(comments::normalize_doc_ref_commas(raw))
                .with_source_offset(node.start_byte())
        } else {
            Block::leaf(raw.to_string()).with_source_offset(node.start_byte())
        }
    }
}

pub(in crate::formatter::build) fn file_level_javadoc<'t>(
    src: &[u8],
    root: tree_sitter::Node<'t>,
) -> Option<tree_sitter::Node<'t>> {
    let mut cursor = root.walk();
    let mut seen_package = false;
    for child in root.children(&mut cursor) {
        if child.kind() == "package_declaration" && !seen_package {
            seen_package = true;
            continue;
        }
        if !matches!(child.kind(), "block_comment" | "line_comment") {
            break;
        }
        if child.kind() != "block_comment" {
            continue;
        }
        let raw = &src[child.start_byte()..child.end_byte().min(src.len())];
        if !raw.starts_with(b"/**") || raw.starts_with(b"/***") {
            continue;
        }
        let owned = child.next_sibling().is_some_and(|n| {
            matches!(
                n.kind(),
                "class_declaration"
                    | "interface_declaration"
                    | "enum_declaration"
                    | "annotation_type_declaration"
                    | "record_declaration"
            )
        });
        if !owned {
            return Some(child);
        }
    }
    None
}

fn javadoc_owner_kind(node: tree_sitter::Node) -> crate::formatter::build::comments::JdOwner {
    use crate::formatter::build::comments::JdOwner;
    let parent = match node.parent() {
        Some(p) => p,
        None => return JdOwner::Method,
    };
    let owner = match parent.kind() {
        "modifiers" => parent.parent(),
        "class_body"
        | "interface_body"
        | "enum_body"
        | "enum_body_declarations"
        | "annotation_type_body"
        | "record_declaration_body"
        | "program"
        | "block"
        | "constructor_body"
        | "switch_block"
        | "switch_block_statement_group" => {
            let mut sib = node.next_sibling();
            loop {
                match sib {
                    Some(s) if matches!(s.kind(), "line_comment" | "block_comment") => {
                        sib = s.next_sibling()
                    }
                    other => break other,
                }
            }
        }
        _ => Some(parent),
    };
    match owner.map(|n| n.kind()) {
        Some("method_declaration")
        | Some("constructor_declaration")
        | Some("compact_constructor_declaration")
        | Some("annotation_type_element_declaration") => JdOwner::Method,
        Some("class_declaration")
        | Some("interface_declaration")
        | Some("enum_declaration")
        | Some("record_declaration")
        | Some("annotation_type_declaration") => JdOwner::Class,
        Some(_) => JdOwner::Other,
        None => JdOwner::Method,
    }
}

fn is_orphan_doc_comment(src: &[u8], node: tree_sitter::Node) -> bool {
    let parent = match node.parent() {
        Some(p) => p,
        None => return false,
    };
    if matches!(
        parent.kind(),
        "block" | "constructor_body" | "switch_block" | "switch_block_statement_group"
    ) {
        let mut sib = node.next_sibling();
        while let Some(s) = sib {
            match s.kind() {
                "line_comment" | "block_comment" => sib = s.next_sibling(),
                "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration" => return false,
                _ => return true,
            }
        }
        return true;
    }
    if !matches!(
        parent.kind(),
        "class_body"
            | "interface_body"
            | "enum_body"
            | "enum_body_declarations"
            | "annotation_type_body"
            | "record_declaration_body"
    ) {
        return false;
    }
    let mut sib = node.next_sibling();
    while let Some(s) = sib {
        match s.kind() {
            "line_comment" | "block_comment" => {
                if is_doc_comment_token(src, s) {
                    return true;
                }
                sib = s.next_sibling();
            }
            "}" => return true,
            _ => return false,
        }
    }
    true
}

fn is_doc_comment_token(src: &[u8], node: tree_sitter::Node) -> bool {
    let raw = &src[node.start_byte()..node.end_byte().min(src.len())];
    match node.kind() {
        "block_comment" => raw.starts_with(b"/**") && !raw.starts_with(b"/***"),
        "line_comment" => raw.starts_with(b"///"),
        _ => false,
    }
}

fn is_owned_markdown_doc(src: &[u8], node: tree_sitter::Node) -> bool {
    let parent = match node.parent() {
        Some(p) => p,
        None => return false,
    };
    let mut end = node;
    while let Some(next) = end.next_sibling() {
        if next.kind() == "line_comment"
            && next.start_position().row == end.end_position().row + 1
            && src[next.start_byte()..].starts_with(b"///")
        {
            end = next;
        } else {
            break;
        }
    }
    {
        let mut start = node;
        while let Some(prev) = start.prev_sibling() {
            if prev.kind() == "line_comment"
                && prev.end_position().row + 1 == start.start_position().row
                && src[prev.start_byte()..].starts_with(b"///")
            {
                start = prev;
            } else {
                break;
            }
        }
        let prefix = &src[..start.start_byte().min(src.len())];
        for line in prefix.split(|&b| b == b'\n') {
            let text = match std::str::from_utf8(line) {
                Ok(t) => t.trim(),
                Err(_) => continue,
            };
            if text.starts_with("///") && format_markdown_doc_line(text) != text {
                return false;
            }
        }
    }
    let mut sib = end.next_sibling();
    while let Some(s) = sib {
        match s.kind() {
            "line_comment" | "block_comment" => {
                if s.kind() == "block_comment" && is_doc_comment_token(src, s) {
                    return false;
                }
                sib = s.next_sibling();
            }
            "}" => return false,
            _ => {
                return !matches!(
                    parent.kind(),
                    "block" | "constructor_body" | "switch_block" | "switch_block_statement_group"
                ) || matches!(
                    s.kind(),
                    "class_declaration"
                        | "interface_declaration"
                        | "enum_declaration"
                        | "record_declaration"
                        | "annotation_type_declaration"
                );
            }
        }
    }
    matches!(parent.kind(), "program" | "modifiers")
}

fn md_run_linked(src: &[u8], a: Node, b: Node) -> bool {
    a.kind() == "line_comment"
        && b.kind() == "line_comment"
        && a.end_position().row + 1 == b.start_position().row
        && src[a.start_byte()..].starts_with(b"///")
        && src[b.start_byte()..].starts_with(b"///")
}

fn markdown_doc_content(line: &str) -> Option<&str> {
    let tag_start = line[3..]
        .find(|c| c != ' ' && c != '\t')
        .map_or(line.len(), |i| i + 3);
    if tag_start == line.len() || line[tag_start..].starts_with('@') {
        return None;
    }
    Some(line.strip_prefix("/// ").unwrap_or(&line[3..]))
}

fn format_markdown_doc_line(raw: &str) -> String {
    let line = raw.trim();
    let tag_start = line[3..]
        .find(|c| c != ' ' && c != '\t')
        .map_or(line.len(), |i| i + 3);
    let content = if line[tag_start..].starts_with('@') {
        &line[tag_start..]
    } else if let Some(rest) = line.strip_prefix("/// ") {
        rest
    } else {
        &line[3..]
    };
    format!(
        "///{}{}",
        if content.is_empty() { "" } else { " " },
        content
    )
}

fn glue_markdown_comparison(line: &str) -> String {
    fn is_gt(w: &str) -> bool {
        w.starts_with('>') && w.chars().all(|c| c == '>' || c == '=')
    }
    fn operand_end(w: &str) -> bool {
        w.chars()
            .last()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | ')' | ']'))
    }
    fn operand_start(w: &str) -> bool {
        w.chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '(' | '"'))
    }

    let mut toks: Vec<(&str, &str)> = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < line.len() {
        let ws = i;
        while i < line.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let word = i;
        while i < line.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
            i += 1;
        }
        toks.push((&line[ws..word], &line[word..i]));
    }

    let glued: Vec<bool> = (0..toks.len())
        .map(|k| {
            k > 0
                && k + 1 < toks.len()
                && is_gt(toks[k].1)
                && !toks[k].0.is_empty()
                && !toks[k + 1].0.is_empty()
                && operand_end(toks[k - 1].1)
                && operand_start(toks[k + 1].1)
        })
        .collect();
    if !glued.iter().any(|g| *g) {
        return line.to_string();
    }

    let mut out = String::with_capacity(line.len());
    for (k, (ws, word)) in toks.iter().enumerate() {
        if !(glued[k] || (k > 0 && glued[k - 1])) {
            out.push_str(ws);
        }
        out.push_str(word);
    }
    out
}

fn is_doc_comment_position(node: tree_sitter::Node) -> bool {
    let parent = match node.parent() {
        Some(p) => p,
        None => return true,
    };
    let mut prev = node.prev_sibling();
    if parent.kind() == "modifiers" {
        while let Some(p) = prev {
            if !matches!(p.kind(), "line_comment" | "block_comment") {
                return false;
            }
            prev = p.prev_sibling();
        }
        return true;
    }
    while let Some(p) = prev {
        match p.kind() {
            "line_comment" | "block_comment" => prev = p.prev_sibling(),
            "modifiers" => return false,
            _ => return true,
        }
    }
    true
}
