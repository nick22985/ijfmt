use super::super::flatten::{Anchor, LeafEntry};
use super::EmitConfig;
use super::comments::{is_bare_block_comment, looks_like_commented_code, source_ws_unchanged};
use super::indentation::{measure_indent_cols, source_column, source_leading_whitespace};
use super::{strip_trailing_spaces, strip_trailing_ws};

pub(super) fn text_block_enforced(i: usize, leaves: &[LeafEntry]) -> usize {
    if !leaves[i].text.starts_with("\"\"\"") || leaves[i].white_space.contains_line_feeds() {
        return 0;
    }
    let ctx = match &leaves[i].anchor {
        Some(Anchor::Arg(c)) if c.smart => c.clone(),
        _ => return 0,
    };
    let belongs = |j: usize| -> bool {
        let mut cur = match &leaves[j].anchor {
            Some(Anchor::Arg(c)) => Some(c.clone()),
            _ => return false,
        };
        while let Some(c) = cur {
            if c.lparen == ctx.lparen {
                return true;
            }
            cur = c.parent.clone();
        }
        false
    };
    let mut j = ctx.lparen + 1;
    while j < leaves.len() {
        if j != ctx.lparen + 1 && !belongs(j) {
            break;
        }
        if j != i
            && leaves[j].arg_probe_for == Some(ctx.lparen)
            && leaves[j].white_space.contains_line_feeds()
        {
            return 1;
        }
        j += 1;
    }
    0
}

pub(super) fn emit_leaf_text(
    out: &mut String,
    leaf: &LeafEntry,
    indent: &str,
    config: &EmitConfig,
    source: &[u8],
    line_indent: usize,
    extra_continuations: usize,
) {
    let text = &leaf.text;
    if !text.contains('\n') {
        out.push_str(text);
        return;
    }
    if text.starts_with("\"\"\"") {
        emit_text_block(out, text, config, line_indent, extra_continuations);
        return;
    }

    let source_ws = leaf
        .source_offset
        .and_then(|offset| std::str::from_utf8(source_leading_whitespace(source, offset)).ok());

    if is_bare_block_comment(text) {
        let emitted_ws = out.rsplit('\n').next().unwrap_or("");
        let starts_line = out.contains('\n') && emitted_ws.chars().all(|c| c == ' ' || c == '\t');
        let lines: Vec<&str> = text.split('\n').collect();
        if starts_line && let Some(source_ws) = source_ws {
            let tab_size = config.indent_size.max(1);
            let indent_of = |ws: &str| -> (usize, usize) {
                let mut spaces = 0usize;
                let mut tabs = 0usize;
                for c in ws.chars() {
                    match c {
                        ' ' => spaces += 1,
                        '\t' => tabs += 1,
                        _ => {}
                    }
                }
                (spaces, tabs)
            };
            let old_ws = source_ws.rsplit('\n').next().unwrap_or("");
            let new_ind = indent_of(emitted_ws);
            let old_ind = indent_of(old_ws);
            out.push_str(lines[0]);
            let shift = (new_ind.0 + new_ind.1 * tab_size) as isize
                - (old_ind.0 + old_ind.1 * tab_size) as isize;
            if new_ind == old_ind {
                for line in &lines[1..] {
                    out.push('\n');
                    out.push_str(line);
                }
            } else {
                for line in &lines[1..] {
                    out.push('\n');
                    let rest = line.trim_start_matches([' ', '\t']);
                    let old_ws = &line[..line.len() - rest.len()];
                    let ws_cols: usize = old_ws
                        .chars()
                        .map(|c| if c == '\t' { tab_size } else { 1 })
                        .sum();
                    let target = ws_cols as isize + shift;
                    let mut new_ws = String::new();
                    if target > 0 {
                        let target = target as usize;
                        if config.use_tabs {
                            new_ws.push_str(&"\t".repeat(target / tab_size));
                            new_ws.push_str(&" ".repeat(target % tab_size));
                        } else {
                            new_ws.push_str(&" ".repeat(target));
                        }
                    }
                    if rest.is_empty() {
                        if new_ws == old_ws {
                            out.push_str(old_ws);
                        }
                        continue;
                    }
                    out.push_str(&new_ws);
                    out.push_str(rest);
                }
            }
        } else {
            out.push_str(lines[0]);
            for line in &lines[1..] {
                out.push('\n');
                out.push_str(line);
            }
        }
        return;
    }

    let ws_unchanged = source_ws_unchanged(out, source_ws);

    let lines: Vec<&str> = text.split('\n').collect();
    let glued_commented_code =
        text.starts_with("/**") && out.ends_with('}') && looks_like_commented_code(text);
    out.push_str(lines[0]);

    let base_indent = leaf
        .source_offset
        .map(|offset| source_column(source, offset, config))
        .unwrap_or(0);

    let is_block_comment = text.starts_with("/*");
    let is_javadoc = text.starts_with("/**");
    let effective_indent = if glued_commented_code {
        let total_cols = if config.use_tabs {
            measure_indent_cols(indent, config).saturating_sub(config.indent_size)
        } else {
            measure_indent_cols(indent, config)
        };
        if config.use_tabs {
            let tab_size = config.indent_size.max(1);
            "\t".repeat(total_cols / tab_size) + &" ".repeat(total_cols % tab_size)
        } else {
            " ".repeat(total_cols)
        }
    } else {
        indent.to_string()
    };

    for line in &lines[1..] {
        if is_javadoc && !leaf.jd_regenerated && ws_unchanged {
            strip_trailing_spaces(out);
        } else {
            strip_trailing_ws(out);
        }
        out.push('\n');

        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }

        let line_indent = measure_indent_cols(line, config);
        let mut extra_cols = line_indent.saturating_sub(base_indent);

        if is_block_comment && extra_cols == 0 {
            if trimmed.starts_with('*') {
                let has_star_prefix_lines = is_javadoc
                    || lines[1..lines.len().saturating_sub(1)]
                        .iter()
                        .any(|l| l.trim_start().starts_with('*'));
                if !trimmed.starts_with("*/") || has_star_prefix_lines {
                    extra_cols = 1;
                }
            } else if is_javadoc {
                extra_cols = 1;
            }
        }

        if is_block_comment && !is_javadoc && extra_cols > 1 && trimmed.starts_with("*/") {
            let has_star_prefix_lines = lines[1..lines.len().saturating_sub(1)]
                .iter()
                .any(|l| l.trim_start().starts_with('*'));
            if has_star_prefix_lines {
                extra_cols = 1;
            }
        }

        if is_javadoc && extra_cols > 1 {
            extra_cols = 1;
        }

        if ws_unchanged && !is_javadoc && !text.starts_with("///") && !trimmed.starts_with('*') {
            out.push_str(line);
            continue;
        }

        out.push_str(&effective_indent);
        if extra_cols > 0 {
            if config.use_tabs && is_block_comment && !is_javadoc {
                let tab_size = config.indent_size.max(1);
                out.push_str(&"\t".repeat(extra_cols / tab_size));
                out.push_str(&" ".repeat(extra_cols % tab_size));
            } else {
                out.push_str(&" ".repeat(extra_cols));
            }
        }
        out.push_str(trimmed);
    }
}

fn emit_text_block(
    out: &mut String,
    text: &str,
    config: &EmitConfig,
    line_indent: usize,
    extra_continuations: usize,
) {
    let total_cols = line_indent + config.continuation_indent * (1 + extra_continuations);
    let indent = if config.use_tabs {
        let tab_size = config.indent_size.max(1);
        "\t".repeat(total_cols / tab_size) + &" ".repeat(total_cols % tab_size)
    } else {
        " ".repeat(total_cols)
    };
    let mut lines = text.split('\n');
    out.push_str(lines.next().unwrap_or(""));
    let indentation = |line: &str| {
        line.bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t' | 12))
            .count()
    };
    let common = lines
        .clone()
        .filter(|line| indentation(line) != line.len())
        .map(indentation)
        .min()
        .unwrap_or(0);
    for line in lines {
        out.push('\n');
        out.push_str(&indent);
        if indentation(line) != line.len() {
            out.push_str(&line[indentation(line).min(common)..]);
        }
    }
}

pub(super) fn emit_raw_source(out: &mut String, text: &str) {
    out.push_str(text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatter::flatten::flatten;
    use crate::formatter::ir::Block;

    fn render(text: &str, source: &[u8], offset: Option<usize>, indent: usize) -> String {
        let config = EmitConfig {
            indent_size: 4,
            continuation_indent: 8,
            use_tabs: false,
            keep_first_column_comment: true,
            align_multiline_ternary: false,
        };
        let mut block = Block::leaf(text);
        block.source_offset = offset;
        let mut leaves = Vec::new();
        flatten(&block, &mut leaves);
        let indent_text = " ".repeat(indent);
        let mut output = format!("\n{indent_text}");
        emit_leaf_text(
            &mut output,
            &leaves[0],
            &indent_text,
            &config,
            source,
            indent,
            0,
        );
        output
    }

    #[test]
    fn single_line_leaves_preserve_text_without_source_context() {
        for text in ["name", "é🙂", "\"/* literal */\"", "// comment"] {
            assert_eq!(render(text, &[], None, 4), format!("\n    {text}"));
        }
    }

    #[test]
    fn multiline_comment_uses_the_leafs_source_whitespace() {
        let text = "/* alpha\n       beta */";
        let source = format!("    {text}");
        assert_eq!(
            render(text, source.as_bytes(), Some(4), 4),
            format!("\n    {text}"),
        );
        assert_eq!(
            render(text, source.as_bytes(), Some(4), 8),
            "\n        /* alpha\n           beta */",
        );
    }
}
