use super::super::flatten::LeafEntry;
use super::{change_touches_last_line, spans_match};

pub(super) fn trailing_comment_source_run<'a>(
    leaves: &[LeafEntry],
    i: usize,
    src: &'a [u8],
    out: &str,
) -> Option<&'a str> {
    let leaf = leaves.get(i)?;
    let off = leaf.source_offset?;
    if off > src.len() || i == 0 {
        return None;
    }
    let end = off.checked_add(leaf.text.len())?;
    if end > src.len() || &src[off..end] != leaf.text.as_bytes() {
        return None;
    }
    let mut b = off;
    while b > 0 && matches!(src[b - 1], b' ' | b'\t') {
        b -= 1;
    }
    if b == 0 || matches!(src[b - 1], b'\n' | b'\r') {
        return None;
    }
    if out.as_bytes().last() != Some(&src[b - 1]) {
        return None;
    }
    std::str::from_utf8(&src[b..off]).ok()
}

pub(super) fn emit_leading_lines(
    out: &mut String,
    last_text_end: &mut usize,
    leaf: &LeafEntry,
    src: &[u8],
    config: &super::EmitConfig,
) {
    use super::indentation::source_column;
    use super::strip_trailing_ws_guarded;
    let ws = &leaf.white_space;
    let blank_contents: Vec<String> = if ws.line_feeds >= 2
        && is_bare_block_comment(&leaf.text)
        && (looks_like_commented_code(&leaf.text)
            || leaf.statement_comment
            || leaf.source_offset.is_some_and(|off| {
                comment_body_hangs(
                    &leaf.text,
                    source_column(src, off, config),
                    config.indent_size.max(1),
                ) && comment_binds_to_nothing(src, off + leaf.text.len())
            })) {
        leaf.source_offset
            .map(|off| {
                let mut s = off.min(src.len());
                while s > 0 && matches!(src[s - 1], b' ' | b'\t' | b'\n' | b'\r') {
                    s -= 1;
                }
                let text = String::from_utf8_lossy(&src[s..off.min(src.len())]);
                let mut lines: Vec<&str> = text.split('\n').collect();
                if lines.len() >= 2 {
                    lines.pop();
                    lines.remove(0);
                } else {
                    lines.clear();
                }
                lines
                    .iter()
                    .map(|l| l.trim_end_matches('\r').to_string())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    for k in 0..ws.line_feeds {
        strip_trailing_ws_guarded(out, *last_text_end);
        out.push('\n');
        if k + 1 < ws.line_feeds
            && let Some(content) = blank_contents.get(k)
        {
            out.push_str(content);
            *last_text_end = out.len();
        }
    }
}

pub(super) fn line_comment_trailing_ws<'a>(
    leaves: &[LeafEntry],
    i: usize,
    src: &'a [u8],
    emitted: &str,
) -> Option<&'a str> {
    let prev = leaves.get(i.checked_sub(1)?)?;
    if !prev.text.starts_with("//") || prev.text.contains('\n') {
        return None;
    }
    let off = prev.source_offset?;
    let end = off.checked_add(prev.text.len())?;
    if end > src.len() || &src[off..end] != prev.text.as_bytes() {
        return None;
    }
    let mut p = end;
    while p < src.len() && matches!(src[p], b' ' | b'\t') {
        p += 1;
    }
    if p == end {
        return None;
    }
    let eb = emitted.as_bytes();
    let cstart_out = eb.len().checked_sub(prev.text.len())?;
    let out_ls = emitted[..cstart_out]
        .rfind('\n')
        .map(|q| q + 1)
        .unwrap_or(0);
    let mut src_ls = off;
    while src_ls > 0 && src[src_ls - 1] != b'\n' {
        src_ls -= 1;
    }
    if !spans_match(&eb[out_ls..cstart_out], &src[src_ls..off]) {
        return None;
    }
    let mut a = cstart_out;
    while a > 0 && matches!(eb[a - 1], b' ' | b'\t' | b'\n') {
        a -= 1;
    }
    let mut b = off;
    while b > 0 && matches!(src[b - 1], b' ' | b'\t' | b'\n' | b'\r') {
        b -= 1;
    }
    let new_run = &eb[a..cstart_out];
    let old_run: Vec<u8> = src[b..off]
        .iter()
        .copied()
        .filter(|&c| c != b'\r')
        .collect();
    if change_touches_last_line(&old_run, new_run) {
        return None;
    }
    std::str::from_utf8(&src[end..p]).ok()
}

pub(super) fn source_ws_unchanged(out: &str, source_ws: Option<&str>) -> bool {
    let Some(ws) = source_ws else { return false };
    let emitted = out.rsplit('\n').next().unwrap_or("");
    !emitted.is_empty() && emitted == ws
}

pub(super) fn comment_binds_to_nothing(src: &[u8], end: usize) -> bool {
    let mut i = end.min(src.len());
    while i < src.len() && matches!(src[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    if i >= src.len() {
        return true;
    }
    src[i] == b'}' || (src[i] == b'/' && i + 1 < src.len() && matches!(src[i + 1], b'/' | b'*'))
}

pub(super) fn comment_body_hangs(text: &str, start_col: usize, tab_size: usize) -> bool {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() < 3 {
        return false;
    }
    let end = lines.len() - 1;
    lines[1..end].iter().any(|l| {
        let t = l.trim();
        if t.is_empty() {
            return false;
        }
        let mut col = 0usize;
        for c in l.chars() {
            match c {
                '\t' => col = (col / tab_size + 1) * tab_size,
                ' ' => col += 1,
                _ => break,
            }
        }
        col > start_col
    })
}

pub(super) fn comment_keeps_first_column(src: &[u8], offset: usize) -> bool {
    offset == 0 || !matches!(src[offset - 1], b' ' | b'\t')
}

pub(super) fn is_bare_block_comment(text: &str) -> bool {
    if !text.starts_with("/*") || !text.contains('\n') {
        return false;
    }
    if text.starts_with("/**") {
        return false;
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let end = lines.len().saturating_sub(1);
    let star_line = |l: &str| l.trim_start().starts_with('*');
    let last = lines[end];
    let last_body = match last.rfind("*/") {
        Some(i) => &last[..i],
        None => last,
    };
    let mut body = lines[1..end]
        .iter()
        .chain(std::iter::once(&last_body))
        .filter(|l| !l.trim().is_empty())
        .peekable();
    if body.peek().is_none() {
        return true;
    }
    body.any(|l| !star_line(l))
}

pub(super) fn looks_like_commented_code(text: &str) -> bool {
    let lines: Vec<&str> = text.split('\n').collect();

    if lines.len() <= 2 {
        let content = text.trim_start_matches("/*").trim_end_matches("*/");
        return content.contains(';');
    }

    let end = lines.len().saturating_sub(1);
    let internal = &lines[1..end];

    if internal
        .iter()
        .any(|l| l.contains(';') && !line_reads_as_prose(l))
    {
        return true;
    }

    if internal.iter().any(|line| {
        let trimmed = line.trim_start();
        let Some((callee, _)) = trimmed.split_once('(') else {
            return false;
        };
        callee.contains('.')
            && !callee.contains(char::is_whitespace)
            && callee
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'.'))
    }) {
        return true;
    }

    let indents: Vec<usize> = internal
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .collect();
    if indents.len() > 1 {
        let first = indents[0];
        if indents.iter().any(|&i| i != first) {
            return true;
        }
    }

    false
}

pub(super) fn line_reads_as_prose(line: &str) -> bool {
    let mut stripped = String::with_capacity(line.len());
    let mut in_str = false;
    let mut esc = false;
    for ch in line.chars() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        if ch == '"' {
            in_str = true;
            stripped.push('_');
            continue;
        }
        stripped.push(ch);
    }
    if stripped.contains('=') || stripped.contains('{') || stripped.contains('}') {
        return false;
    }
    let mut words = stripped.split_whitespace();
    let first = words
        .next()
        .unwrap_or("")
        .trim_end_matches([',', '.', ')', ';', ':']);
    const OPENERS: [&str; 39] = [
        "public",
        "private",
        "protected",
        "static",
        "final",
        "abstract",
        "class",
        "interface",
        "enum",
        "record",
        "return",
        "throw",
        "throws",
        "new",
        "if",
        "else",
        "for",
        "while",
        "do",
        "switch",
        "case",
        "default",
        "break",
        "continue",
        "import",
        "package",
        "try",
        "catch",
        "finally",
        "synchronized",
        "void",
        "int",
        "long",
        "double",
        "float",
        "boolean",
        "char",
        "byte",
        "short",
    ];
    if OPENERS.contains(&first) {
        return false;
    }
    let mut run = 0usize;
    for tok in stripped.split_whitespace() {
        let word = tok.trim_end_matches([',', '.', ')', ';', ':']);
        if !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase()) {
            run += 1;
            if run >= 4 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatter::emit::EmitConfig;
    use crate::formatter::flatten::flatten;
    use crate::formatter::ir::Block;

    #[test]
    fn only_position_preserved_comments_keep_blank_line_bytes() {
        let config = EmitConfig {
            indent_size: 4,
            continuation_indent: 8,
            use_tabs: true,
            keep_first_column_comment: true,
            align_multiline_ternary: false,
        };
        let src = b"a\n \t\n/* code;\n code;*/";
        for (text, expected) in [("/* code;\n code;*/", "a\n \t\n"), ("value", "a\n\n")] {
            let root = Block::leaf(text).with_source_offset(5);
            let mut leaves = Vec::new();
            flatten(&root, &mut leaves);
            leaves[0].white_space.line_feeds = 2;
            let mut out = "a  ".to_string();
            let mut last_text_end = 1;
            emit_leading_lines(&mut out, &mut last_text_end, &leaves[0], src, &config);
            assert_eq!(out, expected);
        }
    }
}
