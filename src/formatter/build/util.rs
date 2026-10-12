use super::super::ir::*;
use crate::config::WrapMode;

pub fn wrap_type_from_mode(mode: WrapMode) -> WrapType {
    match mode {
        WrapMode::DoNotWrap => WrapType::None,
        WrapMode::WrapIfLong => WrapType::Normal,
        WrapMode::ChopIfLong => WrapType::ChopIfNeeded,
        WrapMode::WrapAlways => WrapType::Always,
    }
}

pub fn is_glued_lambda_param_list(node: tree_sitter::Node) -> bool {
    let lambda = match node.parent() {
        Some(p) if p.kind() == "lambda_expression" => p,
        _ => return false,
    };
    match lambda.parent() {
        Some(a) if a.kind() == "argument_list" => {
            a.start_position().row == lambda.start_position().row
        }
        _ => false,
    }
}

pub fn subtree_has_error(node: tree_sitter::Node) -> bool {
    if !node.has_error() {
        return false;
    }
    if node.is_error() {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(subtree_has_error)
}

pub fn has_uncovered_source(node: tree_sitter::Node, src: &[u8]) -> bool {
    let end = node.end_byte().min(src.len());
    let mut pos = node.start_byte().min(end);
    let mut cursor = node.walk();
    let mut any_child = false;
    for child in node.children(&mut cursor) {
        any_child = true;
        let cs = child.start_byte().min(end);
        if cs > pos && src[pos..cs].iter().any(|b| !b.is_ascii_whitespace()) {
            return true;
        }
        if has_uncovered_source(child, src) {
            return true;
        }
        pos = child.end_byte().min(end).max(pos);
    }
    any_child && src[pos..end].iter().any(|b| !b.is_ascii_whitespace())
}
