use super::super::ir::IndentType;
use super::EmitConfig;

pub(super) fn fill_indent(total: usize, stack: &[IndentType], config: &EmitConfig) -> String {
    if !config.use_tabs {
        return " ".repeat(total);
    }
    let ts = config.indent_size.max(1);
    let structural: usize = stack
        .iter()
        .map(|t| match t {
            IndentType::Normal => config.indent_size,
            IndentType::Continuation => config.continuation_indent,
            _ => 0,
        })
        .sum();
    let has_spaces = stack.iter().any(|t| matches!(t, IndentType::Spaces(_)));
    let tabs = if has_spaces {
        total.min(structural) / ts
    } else {
        total / ts
    };
    "\t".repeat(tabs) + &" ".repeat(total - tabs * ts)
}

pub(super) fn line_indent_part(out: &str, tab_size: usize) -> usize {
    out.rsplit('\n')
        .next()
        .unwrap_or("")
        .bytes()
        .take_while(|b| *b == b'\t')
        .count()
        * tab_size.max(1)
}

pub(super) fn source_column(src: &[u8], offset: usize, config: &EmitConfig) -> usize {
    let tab_size = config.indent_size.max(1);
    let line_start = src[..offset]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut col = 0;
    for &b in &src[line_start..offset] {
        if b == b'\t' {
            col = (col / tab_size + 1) * tab_size;
        } else {
            col += 1;
        }
    }
    col
}

pub(super) fn measure_indent_cols(line: &str, config: &EmitConfig) -> usize {
    let tab_size = config.indent_size.max(1);
    let mut col = 0;
    for ch in line.chars() {
        if ch == ' ' {
            col += 1;
        } else if ch == '\t' {
            col = (col / tab_size + 1) * tab_size;
        } else {
            break;
        }
    }
    col
}

pub(super) fn compute_alignment_string(
    stack: &[IndentType],
    anchor_col: usize,
    config: &EmitConfig,
) -> String {
    compute_alignment_string_ex(stack, anchor_col, config, false)
}

pub(super) fn compute_alignment_string_ex(
    stack: &[IndentType],
    anchor_col: usize,
    config: &EmitConfig,
    cont_structural: bool,
) -> String {
    if config.use_tabs {
        let mut structural_cols = 0usize;
        for indent_type in stack {
            match indent_type {
                IndentType::Normal => structural_cols += config.indent_size,
                IndentType::Continuation if cont_structural => {
                    structural_cols += config.continuation_indent
                }
                IndentType::Spaces(n) => structural_cols += n,
                _ => {}
            }
        }
        let tab_size = config.indent_size.max(1);
        let structural_tabs = structural_cols / tab_size;
        let structural_rem = structural_cols % tab_size;
        let structural_total = structural_tabs * tab_size + structural_rem;

        let align_spaces = anchor_col.saturating_sub(structural_total);
        "\t".repeat(structural_tabs) + &" ".repeat(structural_rem + align_spaces)
    } else {
        " ".repeat(anchor_col)
    }
}

pub(super) fn compute_indent_string(stack: &[IndentType], config: &EmitConfig) -> String {
    let mut structural_spaces = 0usize;
    let mut alignment_spaces = 0usize;

    for indent_type in stack {
        match indent_type {
            IndentType::None => {}
            IndentType::Normal => {
                structural_spaces += config.indent_size;
            }
            IndentType::Continuation => {
                structural_spaces += config.continuation_indent;
            }
            IndentType::Spaces(n) => {
                alignment_spaces += n;
            }
            IndentType::BlockReset
            | IndentType::BlockResetKeepOne
            | IndentType::CancelOneContinuation
            | IndentType::CancelOneNormal
            | IndentType::ColumnZero
            | IndentType::NormalIfWrapped(_)
            | IndentType::ContinuationIfWrapped(_)
            | IndentType::LambdaParamGlued => {}
        }
    }

    if config.use_tabs {
        let tab_size = config.indent_size.max(1);
        let tabs = structural_spaces / tab_size;
        let rem = structural_spaces % tab_size;
        "\t".repeat(tabs) + &" ".repeat(rem + alignment_spaces)
    } else {
        " ".repeat(structural_spaces + alignment_spaces)
    }
}

pub(super) fn compute_indent_cols(stack: &[IndentType], config: &EmitConfig) -> usize {
    let mut cols = 0usize;
    for indent_type in stack {
        match indent_type {
            IndentType::Normal => cols += config.indent_size,
            IndentType::Continuation => cols += config.continuation_indent,
            IndentType::Spaces(n) => cols += n,
            _ => {}
        }
    }
    cols
}

pub(super) fn source_leading_whitespace(src: &[u8], offset: usize) -> &[u8] {
    let line_start = src[..offset]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    &src[line_start..offset]
}
