mod build;
mod columns;
mod emit;
mod flatten;
mod ir;
mod lines;
mod parse;
mod source_whitespace;
mod wrap;

#[cfg(test)]
mod tests;

use crate::config::StyleConfig;
use build::BlockBuilder;
use emit::{EmitConfig, emit_with_indents};
use flatten::flatten;
pub use lines::LineSelection;
use source_whitespace::relax_comment_blanks;
use wrap::{IndentOracle, WrapProcessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MalformedJavaMode {
    Preserve,
    Repair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    pub malformed_java: MalformedJavaMode,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            malformed_java: MalformedJavaMode::Preserve,
        }
    }
}

pub fn format(source: &str, cfg: &StyleConfig) -> anyhow::Result<String> {
    format_with_options(source, cfg, FormatOptions::default())
}

pub fn format_with_options(
    source: &str,
    cfg: &StyleConfig,
    options: FormatOptions,
) -> anyhow::Result<String> {
    let had_crlf = source.contains("\r\n");
    let normalized;
    let source: &str = if had_crlf {
        normalized = source.replace("\r\n", "\n");
        &normalized
    } else {
        source
    };

    let parse::ParsedJava { tree, depth } = parse::parse(source)?;
    if depth <= 64 {
        return format_parsed(source, cfg, options, &tree, had_crlf);
    }
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("ijfmt-deep-format".to_owned())
            .stack_size(depth * (64 << 10))
            .spawn_scoped(scope, || {
                format_parsed(source, cfg, options, &tree, had_crlf)
            })?
            .join()
            .map_err(|_| anyhow::anyhow!("formatter worker panicked"))?
    })
}

pub fn format_lines(
    source: &str,
    cfg: &StyleConfig,
    options: FormatOptions,
    selection: &LineSelection,
) -> anyhow::Result<String> {
    if selection.is_empty() {
        return Ok(source.to_string());
    }
    let formatted = format_with_options(source, cfg, options)?;
    Ok(lines::apply_to_selected(source, &formatted, selection))
}

fn format_parsed(
    source: &str,
    cfg: &StyleConfig,
    options: FormatOptions,
    tree: &tree_sitter::Tree,
    had_crlf: bool,
) -> anyhow::Result<String> {
    let root = tree.root_node();
    if std::env::var("IJFMT_TREE").is_ok() {
        eprintln!("{}", root.to_sexp());
    }
    let builder = BlockBuilder::new_with_options(source.as_bytes(), cfg, options.malformed_java);
    let (mut block_tree, wraps, alignment_parents, positional_wraps) = builder.build(root);
    relax_comment_blanks(
        &mut block_tree,
        source.as_bytes(),
        cfg.keep_blank_lines_in_code,
    );

    let emit_cfg = EmitConfig {
        indent_size: cfg.indent_size,
        continuation_indent: cfg.continuation_indent,
        use_tabs: cfg.use_tabs,
        keep_first_column_comment: cfg.keep_first_column_comment,
        align_multiline_ternary: cfg.align_multiline_ternary,
    };
    let wrap_parents = crate::formatter::wrap::collect_wrap_parents(&block_tree);

    const MAX_REFINEMENT_PASSES: usize = 30;
    let mut oracle: Option<IndentOracle> = None;
    let mut out = String::new();
    let mut leaves = Vec::new();
    for pass in 0..=MAX_REFINEMENT_PASSES {
        flatten(&block_tree, &mut leaves);
        let mut processor =
            WrapProcessor::new(cfg.right_margin, cfg.indent_size, cfg.continuation_indent);
        if let Some(oracle) = oracle.take() {
            processor.set_indent_oracle(oracle);
        }
        processor.process(&mut leaves, &wraps, &positional_wraps, &wrap_parents);
        crate::formatter::flatten::resolve_conditional_indents(&mut leaves);
        let (next_out, full_indents, _) =
            emit_with_indents(&leaves, &emit_cfg, source.as_bytes(), &alignment_parents);
        let converged = pass > 0 && next_out == out;
        out = next_out;
        if converged || pass == MAX_REFINEMENT_PASSES {
            break;
        }

        let (_, indents, started) = crate::formatter::emit::emit_with_indents_mode(
            &leaves,
            &emit_cfg,
            source.as_bytes(),
            &alignment_parents,
            true,
        );
        oracle = Some(IndentOracle::from_emission(full_indents, indents, &started));
    }
    drop(leaves);

    Ok(source_whitespace::restore_file_whitespace(
        out,
        source,
        cfg.keep_blank_lines_in_declarations,
        had_crlf,
    ))
}
