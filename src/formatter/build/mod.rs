mod comments;
mod declarations;
mod expressions;
mod fallback;
mod imports;
mod program;
mod statements;
mod util;

use std::collections::HashMap;
use tree_sitter::Node;

use super::MalformedJavaMode;
use super::ir::*;
use crate::config::StyleConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MalformedSupertypeRecovery {
    Scoped,
    Generic,
    Simple,
}

pub struct BlockBuilder<'a> {
    pub(crate) src: &'a [u8],
    pub(crate) cfg: &'a StyleConfig,
    pub(crate) malformed_java: MalformedJavaMode,
    pub(crate) malformed_supertype_recovery: Option<MalformedSupertypeRecovery>,
    pub(crate) ids: IdGen,
    pub(crate) wraps: HashMap<WrapId, Wrap>,
    pub(crate) positional_wraps: std::collections::HashSet<WrapId>,
    pub(crate) enclosing_arg_wrap: Option<WrapId>,
    pub(crate) binary_ctx_wrap: Option<WrapId>,
    pub(crate) nonbin_enclosing_wrap: Option<WrapId>,
    pub(crate) alignment_parents: HashMap<AlignmentId, AlignmentId>,
    pub(crate) for_header_alignment: Option<AlignmentId>,
    pub(crate) last_params_wrap: Option<WrapId>,
    pub(crate) enclosing_cont_start_row: Option<usize>,
    pub(crate) enclosing_arg_lparen_row: Option<usize>,
    pub(crate) in_binary_operand: bool,
    pub(crate) in_binary_rhs: bool,
    pub(crate) binary_parent_sign_at_eol: bool,
    pub(crate) paren_direct_child: bool,
    pub(crate) ternary_depth: usize,
    pub(crate) binary_rhs_operator_row: Option<usize>,
    pub(crate) binary_rhs_operator_on_new_line: bool,
    pub(crate) in_broken_ternary_branch: bool,
    pub(crate) in_ternary_alternative: bool,
    pub(crate) in_same_line_outer_arg: bool,
    pub(crate) force_brace_trailing_comments: Vec<Block>,
    pub(crate) force_brace_trailing_owner: Option<usize>,
    pub(crate) arg_continuation_depth: u32,
    pub(crate) in_single_arg_list: bool,
    pub(crate) enclosing_arg_has_break: bool,
    pub(crate) formatter_off: bool,
    pub(crate) reset_ternary_source_breaks: bool,
    pub(crate) checker_nullable_declarative: bool,
    pub(crate) javadoc_stop_byte: Option<usize>,
}

impl<'a> BlockBuilder<'a> {
    #[cfg(test)]
    pub fn new(src: &'a [u8], cfg: &'a StyleConfig) -> Self {
        Self::new_with_options(src, cfg, MalformedJavaMode::Preserve)
    }

    pub fn new_with_options(
        src: &'a [u8],
        cfg: &'a StyleConfig,
        malformed_java: MalformedJavaMode,
    ) -> Self {
        let checker_nullable_declarative = {
            let bytes_contain =
                |needle: &[u8]| -> bool { src.windows(needle.len()).any(|w| w == needle) };
            bytes_contain(b"import org.checkerframework.checker.nullness.qual.Nullable;")
                && bytes_contain(b"@DefaultQualifier(NonNull.class)")
        };
        BlockBuilder {
            src,
            cfg,
            malformed_java,
            malformed_supertype_recovery: None,
            ids: IdGen::new(),
            wraps: HashMap::new(),
            positional_wraps: std::collections::HashSet::new(),
            enclosing_arg_wrap: None,
            binary_ctx_wrap: None,
            nonbin_enclosing_wrap: None,
            alignment_parents: HashMap::new(),
            for_header_alignment: None,
            last_params_wrap: None,
            enclosing_cont_start_row: None,
            enclosing_arg_lparen_row: None,
            in_binary_operand: false,
            in_binary_rhs: false,
            binary_parent_sign_at_eol: false,
            paren_direct_child: false,
            ternary_depth: 0,
            binary_rhs_operator_row: None,
            binary_rhs_operator_on_new_line: false,
            in_broken_ternary_branch: false,
            in_ternary_alternative: false,
            in_same_line_outer_arg: false,
            force_brace_trailing_comments: Vec::new(),
            force_brace_trailing_owner: None,
            arg_continuation_depth: 0,
            in_single_arg_list: false,
            enclosing_arg_has_break: false,
            formatter_off: false,
            reset_ternary_source_breaks: false,
            checker_nullable_declarative,
            javadoc_stop_byte: None,
        }
    }

    pub fn build(
        mut self,
        root: Node,
    ) -> (
        Block,
        HashMap<WrapId, Wrap>,
        HashMap<AlignmentId, AlignmentId>,
        std::collections::HashSet<WrapId>,
    ) {
        self.javadoc_stop_byte =
            comments::nodes::file_level_javadoc(self.src, root).and_then(|n| {
                let raw =
                    std::str::from_utf8(&self.src[n.start_byte()..n.end_byte()]).unwrap_or("");
                if self.process_javadoc(raw, 0) != raw {
                    Some(n.end_byte())
                } else {
                    None
                }
            });
        let block = self.node(root);
        (
            block,
            self.wraps,
            self.alignment_parents,
            self.positional_wraps,
        )
    }

    pub(crate) fn make_wrap(&mut self, wrap_type: WrapType) -> Wrap {
        let w = self.ids.wrap(wrap_type);
        self.wraps.insert(w.id, w.clone());
        w
    }

    pub(crate) fn text(&self, node: Node) -> &str {
        node.utf8_text(self.src).unwrap_or("")
    }

    pub(crate) fn raw_source_block(&self, node: Node) -> Block {
        let start = node
            .prev_sibling()
            .map_or(node.start_byte(), |prev| prev.end_byte());
        let raw = std::str::from_utf8(&self.src[start..node.end_byte()]).unwrap_or("");
        Block::leaf(raw).with_source_offset(start).with_raw_source()
    }

    pub(crate) fn lbrace_new_line(&self, child: Node) -> Spacing {
        let keep = self.cfg.keep_blank_lines_in_code;
        let mut i = child.start_byte();
        let mut line_feeds = 0usize;
        while i > 0 {
            match self.src[i - 1] {
                b'\n' => {
                    line_feeds += 1;
                    i -= 1;
                }
                b' ' | b'\t' | b'\r' => i -= 1,
                _ => break,
            }
        }
        Spacing {
            min_spaces: 0,
            max_spaces: 0,
            min_line_feeds: line_feeds.min(keep + 1).max(1) as u32,
            keep_blank_lines: keep as u32,
            keep_line_breaks: false,
        }
    }

    fn compute_output_col(&self, node: Node) -> usize {
        let mut depth = 0usize;
        let mut extra_continuation = 0usize;
        let mut cur = node.parent();
        while let Some(p) = cur {
            match p.kind() {
                "class_body"
                | "interface_body"
                | "enum_body"
                | "block"
                | "constructor_body"
                | "switch_block"
                | "annotation_type_body"
                | "record_declaration_body" => {
                    depth += 1;
                }
                _ => {}
            }
            if (p.kind() == "class_body" || p.kind() == "interface_body")
                && let Some(gp) = p.parent()
                && gp.kind() == "enum_constant"
                && self.cfg.class_brace_style == crate::config::BraceStyle::NextLine
            {
                extra_continuation += 1;
            }
            cur = p.parent();
        }
        depth * self.cfg.indent_size + extra_continuation * self.cfg.continuation_indent
    }

    pub(crate) fn source_visual_col(&self, node: Node) -> usize {
        let pos = node.start_position();
        let line_start = node.start_byte() - pos.column;
        let line_bytes = &self.src[line_start..node.start_byte()];
        let tab_size = self.cfg.indent_size.max(1);
        let mut col = 0usize;
        for &b in line_bytes {
            if b == b'\t' {
                col = (col / tab_size + 1) * tab_size;
            } else {
                col += 1;
            }
        }
        col
    }

    pub(crate) fn node(&mut self, node: Node) -> Block {
        match node.kind() {
            "program" => self.program(node),

            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => self.class_decl(node),
            "method_declaration"
            | "constructor_declaration"
            | "compact_constructor_declaration" => self.method_decl(node),

            "argument_list" => self.argument_list(node),
            "record_pattern_body" => self.record_pattern_body(node),
            "binary_expression" => self.binary_expression(node),
            "ternary_expression" => self.ternary_expression(node),
            "assignment_expression" => self.assignment_expression(node),
            "method_invocation" => self.method_invocation(node),
            "lambda_expression" => self.lambda_expression(node),
            "object_creation_expression" => self.object_creation(node),
            "array_creation_expression" => self.array_creation(node),
            "array_initializer" => self.array_initializer(node),

            "formal_parameters" => self.formal_parameters(node),
            "resource" => self.resource(node),

            "field_access" => self.field_access(node),

            "resource_specification" => self.resource_specification(node),

            "block" | "constructor_body" => self.code_block(node),
            "switch_block" => self.switch_block(node),
            "class_body"
            | "interface_body"
            | "enum_body"
            | "annotation_type_body"
            | "record_declaration_body" => self.class_body(node),

            "catch_clause" => self.catch_clause(node),

            "if_statement"
            | "for_statement"
            | "enhanced_for_statement"
            | "while_statement"
            | "do_statement"
            | "try_statement"
            | "try_with_resources_statement"
            | "synchronized_statement"
            | "switch_statement"
            | "switch_expression"
            | "labeled_statement" => self.statement_with_body(node),

            "modifiers" => self.modifiers_and_annotations(node),

            "enum_constant" => self.enum_constant(node),

            "type_parameters" => self.type_parameters(node),

            "string_literal" if self.text(node).starts_with("\"\"\"") => {
                Block::leaf(self.text(node).to_string()).with_source_offset(node.start_byte())
            }

            "annotation_type_declaration" => self.class_decl(node),

            "variable_declarator" => self.variable_declarator(node),

            "field_declaration" | "local_variable_declaration" | "constant_declaration" => {
                self.declaration_statement(node)
            }

            "switch_label" => self.switch_label(node),

            "expression_statement"
            | "return_statement"
            | "throw_statement"
            | "assert_statement"
            | "break_statement"
            | "continue_statement"
            | "yield_statement"
            | "explicit_constructor_invocation"
            | "import_declaration"
            | "package_declaration" => self.simple_statement(node),

            "annotation_type_element_declaration" => self.fallback(node),

            "parenthesized_expression" => self.parenthesized_expression(node),

            "cast_expression" => self.cast_expression(node),

            "element_value_pair" => self.element_value_pair(node),

            "annotation_argument_list" => self.annotation_argument_list(node),

            _ => self.fallback(node),
        }
    }
}
