use super::ir::Block;

fn first_leaf(block: &Block) -> Option<&Block> {
    if block.children.is_empty() {
        return Some(block);
    }
    block.children.first().and_then(first_leaf)
}

fn comment_block_start(block: &Block) -> Option<(usize, bool)> {
    let leaf = first_leaf(block)?;
    let text = leaf.text.trim_start();
    if !text.starts_with("/*") && !text.starts_with("//") {
        return None;
    }
    let c_style = text.starts_with("/*") && !text.starts_with("/**");
    Some((leaf.source_offset?, c_style))
}

fn source_line_feeds_before(src: &[u8], offset: usize) -> u32 {
    let mut i = offset.min(src.len());
    let mut line_feeds = 0u32;
    while i > 0 {
        match src[i - 1] {
            b'\n' => {
                line_feeds += 1;
                i -= 1;
            }
            b' ' | b'\t' | b'\r' => i -= 1,
            _ => break,
        }
    }
    line_feeds
}

pub(super) fn relax_comment_blanks(block: &mut Block, src: &[u8], keep_blank_lines_in_code: usize) {
    for child in block.children.iter_mut() {
        let start = comment_block_start(child);
        if let (Some((offset, c_style)), Some(spacing)) = (start, child.spacing.as_mut())
            && spacing.min_line_feeds >= 1
        {
            let mut feeds = source_line_feeds_before(src, offset);
            if !c_style {
                feeds = feeds.min(keep_blank_lines_in_code as u32 + 1);
            }
            if feeds > spacing.min_line_feeds {
                spacing.min_line_feeds = feeds;
                spacing.keep_blank_lines = spacing.keep_blank_lines.max(feeds - 1);
            }
        }
        relax_comment_blanks(child, src, keep_blank_lines_in_code);
    }
}

pub(super) fn restore_file_whitespace(
    mut out: String,
    source: &str,
    keep_blank_lines: usize,
    had_crlf: bool,
) -> String {
    let first_non_ws = source
        .bytes()
        .position(|b| b != b'\n' && b != b'\r' && b != b' ' && b != b'\t');
    if let Some(pos) = first_non_ws {
        let starts_with_comment =
            source[pos..].starts_with("//") || source[pos..].starts_with("/*");
        if starts_with_comment && pos > 0 {
            let leading_newlines = source[..pos].bytes().filter(|&b| b == b'\n').count();
            let max_keep = keep_blank_lines;
            let kept = leading_newlines.min(max_keep);
            if kept > 0 {
                out = "\n".repeat(kept) + &out;
            }
        }
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }

    {
        let src_tail = &source[source.trim_end().len()..];
        let keep = out.trim_end().len();
        out.truncate(keep);
        out.push_str(src_tail);
    }

    if had_crlf {
        out = out.replace('\n', "\r\n");
    }

    out
}
