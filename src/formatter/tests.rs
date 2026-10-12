use super::emit::{EmitConfig, emit};
use super::flatten::flatten;
use super::ir::*;
use super::wrap::WrapProcessor;
use super::{FormatOptions, MalformedJavaMode, format_with_options};
use crate::config::StyleConfig;
use std::collections::HashMap;

fn default_config() -> EmitConfig {
    EmitConfig {
        indent_size: 4,
        continuation_indent: 8,
        use_tabs: false,
        keep_first_column_comment: true,
        align_multiline_ternary: false,
    }
}

#[test]
fn block_constructors_share_defaults_without_losing_contents() {
    let leaf = Block::leaf("name").with_source_offset(7);
    let node = Block::node(vec![leaf]);
    assert!(node.text.is_empty());
    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].text, "name");
    assert_eq!(node.children[0].source_offset, Some(7));
    assert_eq!(node.source_offset, None);
    assert_eq!(node.indent, IndentType::None);
    assert!(node.wrap.is_none());
    assert!(node.spacing.is_none());
    assert!(node.alignment.is_none());

    let leaf = Block::leaf("other");
    assert_eq!(leaf.text, "other");
    assert!(leaf.children.is_empty());
    assert_eq!(leaf.indent, node.indent);
    assert_eq!(leaf.source_offset, node.source_offset);
    assert!(!leaf.raw_source && !node.raw_source);
}

fn format_block(root: &Block, right_margin: usize) -> String {
    let config = default_config();
    format_block_with_config(root, right_margin, &config)
}

fn format_block_with_config(root: &Block, right_margin: usize, config: &EmitConfig) -> String {
    let mut leaves = Vec::new();
    flatten(root, &mut leaves);
    let mut wraps = HashMap::new();
    collect_wraps(root, &mut wraps);

    let processor =
        WrapProcessor::new(right_margin, config.indent_size, config.continuation_indent);
    let wrap_parents = super::wrap::collect_wrap_parents(root);
    processor.process(
        &mut leaves,
        &wraps,
        &std::collections::HashSet::new(),
        &wrap_parents,
    );
    emit(&leaves, config, &[], &std::collections::HashMap::new())
}

fn collect_wraps(block: &Block, wraps: &mut HashMap<WrapId, Wrap>) {
    if let Some(ref w) = block.wrap {
        wraps.insert(w.id, w.clone());
    }
    for child in &block.children {
        collect_wraps(child, wraps);
    }
}

#[test]
fn flatten_reuses_allocation_without_retaining_previous_pass_state() {
    let root = Block::node(vec![
        Block::leaf("// comment").with_line_comment(),
        Block::leaf("value").with_indent(IndentType::Normal),
    ]);
    let mut leaves = Vec::new();
    flatten(&root, &mut leaves);
    let original = format!("{leaves:?}");
    let allocation = leaves.as_ptr();

    leaves[0].text.push_str(" stale text");
    leaves[0].wraps.push(WrapId(99));
    leaves[1].white_space.line_feeds = 99;
    leaves[1].indent_stack.clear();
    flatten(&root, &mut leaves);

    assert_eq!(leaves.as_ptr(), allocation);
    assert_eq!(format!("{leaves:?}"), original);

    flatten(&Block::leaf("replacement"), &mut leaves);
    assert_eq!(leaves.len(), 1);
    assert_eq!(leaves[0].text, "replacement");
    assert_eq!(leaves.as_ptr(), allocation);
}

#[test]
fn malformed_java_mode_controls_duplicate_interface_repair() {
    let source = "class C implements A,\n    example.B\n    example.B {\n}\n";
    let cfg = StyleConfig::default();

    let preserved = format_with_options(
        source,
        &cfg,
        FormatOptions {
            malformed_java: MalformedJavaMode::Preserve,
        },
    )
    .unwrap();
    let repaired = format_with_options(
        source,
        &cfg,
        FormatOptions {
            malformed_java: MalformedJavaMode::Repair,
        },
    )
    .unwrap();

    let without_whitespace = |formatted: &str| {
        formatted
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
    };

    assert_eq!(
        without_whitespace(&preserved).matches("example.B").count(),
        2
    );
    assert_eq!(
        without_whitespace(&repaired).matches("example.B").count(),
        1
    );
}

#[test]
fn malformed_java_mode_does_not_change_valid_java() {
    let source = "class C implements A { void method() {} }\n";
    let cfg = StyleConfig::default();
    let format = |malformed_java| {
        format_with_options(source, &cfg, FormatOptions { malformed_java }).unwrap()
    };

    assert_eq!(
        format(MalformedJavaMode::Preserve),
        format(MalformedJavaMode::Repair)
    );
}

#[test]
fn test_leaf_only() {
    let root = Block::leaf("hello");
    assert_eq!(format_block(&root, 120), "hello");
}

#[test]
fn test_two_leaves_with_space() {
    let root = Block::node(vec![
        Block::leaf("hello"),
        Block::leaf("world").with_spacing(Spacing::one_space()),
    ]);
    assert_eq!(format_block(&root, 120), "hello world");
}

#[test]
fn test_no_space() {
    let root = Block::node(vec![
        Block::leaf("foo"),
        Block::leaf("(").with_spacing(Spacing::none()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);
    assert_eq!(format_block(&root, 120), "foo()");
}

#[test]
fn test_forced_newline() {
    let root = Block::node(vec![
        Block::leaf("line1;"),
        Block::leaf("line2;").with_spacing(Spacing::new_line()),
    ]);
    assert_eq!(format_block(&root, 120), "line1;\nline2;");
}

#[test]
fn test_normal_indent() {
    let root = Block::node(vec![
        Block::leaf("{"),
        Block::leaf("x = 1;")
            .with_indent(IndentType::Normal)
            .with_spacing(Spacing::new_line()),
        Block::leaf("}").with_spacing(Spacing::new_line()),
    ]);
    assert_eq!(format_block(&root, 120), "{\n    x = 1;\n}");
}

#[test]
fn test_continuation_indent() {
    let root = Block::node(vec![
        Block::leaf("return"),
        Block::leaf("x + y;")
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::new_line()),
    ]);
    assert_eq!(format_block(&root, 120), "return\n        x + y;");
}

#[test]
fn test_nested_indent() {
    let root = Block::node(vec![
        Block::leaf("{"),
        Block::node(vec![
            Block::leaf("if (true)"),
            Block::leaf("{").with_spacing(Spacing::one_space()),
            Block::leaf("x();")
                .with_indent(IndentType::Normal)
                .with_spacing(Spacing::new_line()),
            Block::leaf("}").with_spacing(Spacing::new_line()),
        ])
        .with_indent(IndentType::Normal)
        .with_spacing(Spacing::new_line()),
        Block::leaf("}").with_spacing(Spacing::new_line()),
    ]);
    assert_eq!(
        format_block(&root, 120),
        "{\n    if (true) {\n        x();\n    }\n}"
    );
}

#[test]
fn test_wrap_always() {
    let mut ids = IdGen::new();
    let wrap_id = ids.wrap_id();

    let make_wrap = || Wrap {
        id: wrap_id,
        wrap_type: WrapType::Always,
        wrap_first_element: false,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("foo("),
        Block::leaf("a")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("b")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("c")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);

    let result = format_block(&root, 120);
    assert_eq!(result, "foo(a,\n        b,\n        c)");
}

#[test]
fn test_wrap_normal_fits() {
    let mut ids = IdGen::new();
    let wrap = ids.wrap(WrapType::Normal);

    let root = Block::node(vec![
        Block::leaf("foo("),
        Block::leaf("a")
            .with_wrap(wrap.clone())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("b")
            .with_wrap(wrap)
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);

    assert_eq!(format_block(&root, 120), "foo(a, b)");
}

#[test]
fn test_wrap_normal_overflows() {
    let mut ids = IdGen::new();
    let wrap = ids.wrap(WrapType::Normal);

    let root = Block::node(vec![
        Block::leaf("foo("),
        Block::leaf("aaaa")
            .with_wrap(wrap.clone())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("bbbb")
            .with_wrap(wrap)
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);

    let result = format_block(&root, 12);
    assert_eq!(result, "foo(aaaa,\n        bbbb)");
}

#[test]
fn test_chop_if_needed_fits() {
    let mut ids = IdGen::new();
    let wrap_id = ids.wrap_id();

    let make_wrap = || Wrap {
        id: wrap_id,
        wrap_type: WrapType::ChopIfNeeded,
        wrap_first_element: false,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("foo("),
        Block::leaf("a")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("b")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("c")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);

    assert_eq!(format_block(&root, 120), "foo(a, b, c)");
}

#[test]
fn test_chop_if_needed_overflows() {
    let mut ids = IdGen::new();
    let wrap_id = ids.wrap_id();

    let make_wrap = || Wrap {
        id: wrap_id,
        wrap_type: WrapType::ChopIfNeeded,
        wrap_first_element: false,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("foo("),
        Block::leaf("aaa")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("bbb")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("ccc")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);

    let result = format_block(&root, 14);
    assert_eq!(result, "foo(aaa,\n        bbb,\n        ccc)");
}

#[test]
fn test_parent_wrap_wins() {
    let mut ids = IdGen::new();
    let outer_wrap_id = ids.wrap_id();
    let inner_wrap = ids.child_wrap(outer_wrap_id, WrapType::Normal);

    let make_outer = || Wrap {
        id: outer_wrap_id,
        wrap_type: WrapType::Normal,
        wrap_first_element: false,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("outer("),
        Block::leaf("arg1")
            .with_wrap(make_outer())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::node(vec![
            Block::leaf("inner("),
            Block::leaf("aaaa"),
            Block::leaf(",").with_spacing(Spacing::none()),
            Block::leaf("bbbb")
                .with_wrap(inner_wrap)
                .with_spacing(Spacing::one_space()),
            Block::leaf(")").with_spacing(Spacing::none()),
        ])
        .with_wrap(make_outer())
        .with_indent(IndentType::Continuation)
        .with_spacing(Spacing::one_space()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);

    let result = format_block(&root, 28);
    assert_eq!(result, "outer(arg1,\n        inner(aaaa, bbbb))");
}

#[test]
fn test_ignore_parent_wraps() {
    let mut ids = IdGen::new();
    let outer_wrap_id = ids.wrap_id();
    let inner_wrap_id = ids.wrap_id();

    let make_outer = || Wrap {
        id: outer_wrap_id,
        wrap_type: WrapType::Normal,
        wrap_first_element: false,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let make_inner = || Wrap {
        id: inner_wrap_id,
        wrap_type: WrapType::Normal,
        wrap_first_element: false,
        parent: Some(outer_wrap_id),
        ignore_parent_wraps: true,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("outer("),
        Block::leaf("arg1")
            .with_wrap(make_outer())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::node(vec![
            Block::leaf("inner("),
            Block::leaf("aaaa")
                .with_wrap(make_inner())
                .with_spacing(Spacing::none()),
            Block::leaf(",").with_spacing(Spacing::none()),
            Block::leaf("bbbb")
                .with_wrap(make_inner())
                .with_indent(IndentType::Continuation)
                .with_spacing(Spacing::one_space()),
            Block::leaf(")").with_spacing(Spacing::none()),
        ])
        .with_wrap(make_outer())
        .with_indent(IndentType::Continuation)
        .with_spacing(Spacing::one_space()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);

    let result = format_block(&root, 28);
    assert_eq!(result, "outer(arg1, inner(aaaa,\n                bbbb))");
}

#[test]
fn test_tab_indent() {
    let config = EmitConfig {
        indent_size: 4,
        continuation_indent: 8,
        use_tabs: true,
        keep_first_column_comment: true,
        align_multiline_ternary: false,
    };

    let root = Block::node(vec![
        Block::leaf("{"),
        Block::leaf("x = 1;")
            .with_indent(IndentType::Normal)
            .with_spacing(Spacing::new_line()),
        Block::leaf("}").with_spacing(Spacing::new_line()),
    ]);

    assert_eq!(
        format_block_with_config(&root, 120, &config),
        "{\n\tx = 1;\n}"
    );
}

#[test]
fn test_blank_lines() {
    let root = Block::node(vec![
        Block::leaf("a();"),
        Block::leaf("b();").with_spacing(Spacing::blank_line()),
    ]);
    assert_eq!(format_block(&root, 120), "a();\n\nb();");
}

#[test]
fn test_multiple_spaces() {
    let root = Block::node(vec![
        Block::leaf("a"),
        Block::leaf("=").with_spacing(Spacing::spaces(1, 1)),
        Block::leaf("b").with_spacing(Spacing::spaces(1, 1)),
    ]);
    assert_eq!(format_block(&root, 120), "a = b");
}

#[test]
fn test_method_chain_wrapping() {
    let mut ids = IdGen::new();
    let chain_wrap_id = ids.wrap_id();

    let make_chain_wrap = || Wrap {
        id: chain_wrap_id,
        wrap_type: WrapType::ChopIfNeeded,
        wrap_first_element: false,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("builder").with_wrap(make_chain_wrap()),
        Block::leaf(".method1(a)")
            .with_wrap(make_chain_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(".method2(b)")
            .with_wrap(make_chain_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(".method3(c)")
            .with_wrap(make_chain_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
    ]);

    assert_eq!(
        format_block(&root, 120),
        "builder.method1(a).method2(b).method3(c)"
    );

    let result = format_block(&root, 30);
    assert_eq!(
        result,
        "builder\n        .method1(a)\n        .method2(b)\n        .method3(c)"
    );
}

#[test]
fn test_arg_list_wrap() {
    let mut ids = IdGen::new();
    let arg_wrap_id = ids.wrap_id();

    let make_arg_wrap = || Wrap {
        id: arg_wrap_id,
        wrap_type: WrapType::ChopIfNeeded,
        wrap_first_element: false,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("foo("),
        Block::leaf("longArgument1")
            .with_wrap(make_arg_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("longArgument2")
            .with_wrap(make_arg_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("longArgument3")
            .with_wrap(make_arg_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);

    let result = format_block(&root, 40);
    assert_eq!(
        result,
        "foo(longArgument1,\n        longArgument2,\n        longArgument3)"
    );
}

#[test]
fn test_wrap_first_element() {
    let mut ids = IdGen::new();
    let wid = ids.wrap_id();

    let make_wrap = || Wrap {
        id: wid,
        wrap_type: WrapType::Always,
        wrap_first_element: true,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("foo("),
        Block::leaf("a")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::leaf(",").with_spacing(Spacing::none()),
        Block::leaf("b")
            .with_wrap(make_wrap())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::one_space()),
        Block::leaf(")").with_spacing(Spacing::none()),
    ]);

    let result = format_block(&root, 120);
    assert_eq!(result, "foo(\n        a,\n        b)");
}

#[test]
fn test_chain_with_arg_wrapping() {
    let mut ids = IdGen::new();
    let chain_wrap_id = ids.wrap_id();
    let arg_wrap_id = ids.wrap_id();

    let make_chain = || Wrap {
        id: chain_wrap_id,
        wrap_type: WrapType::Normal,
        wrap_first_element: false,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let make_arg = || Wrap {
        id: arg_wrap_id,
        wrap_type: WrapType::Normal,
        wrap_first_element: false,
        parent: Some(chain_wrap_id),
        ignore_parent_wraps: true,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("obj").with_wrap(make_chain()),
        Block::leaf(".first()")
            .with_wrap(make_chain())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::node(vec![
            Block::leaf(".method("),
            Block::leaf("longArg1")
                .with_wrap(make_arg())
                .with_spacing(Spacing::none()),
            Block::leaf(",").with_spacing(Spacing::none()),
            Block::leaf("longArg2")
                .with_wrap(make_arg())
                .with_indent(IndentType::Continuation)
                .with_spacing(Spacing::one_space()),
            Block::leaf(")").with_spacing(Spacing::none()),
        ])
        .with_wrap(make_chain())
        .with_indent(IndentType::Continuation)
        .with_spacing(Spacing::none()),
    ]);

    let result = format_block(&root, 35);
    assert_eq!(
        result,
        "obj.first().method(longArg1,\n                longArg2)"
    );
}

#[test]
fn test_chain_with_arg_wrapping_no_prefer() {
    let mut ids = IdGen::new();
    let chain_wrap_id = ids.wrap_id();
    let arg_wrap = ids.child_wrap(chain_wrap_id, WrapType::Normal);

    let make_chain = || Wrap {
        id: chain_wrap_id,
        wrap_type: WrapType::Normal,
        wrap_first_element: false,
        parent: None,
        ignore_parent_wraps: false,
        cand_ignore_parents: false,
    };

    let root = Block::node(vec![
        Block::leaf("obj").with_wrap(make_chain()),
        Block::leaf(".first()")
            .with_wrap(make_chain())
            .with_indent(IndentType::Continuation)
            .with_spacing(Spacing::none()),
        Block::node(vec![
            Block::leaf(".method("),
            Block::leaf("longArg1"),
            Block::leaf(",").with_spacing(Spacing::none()),
            Block::leaf("longArg2")
                .with_wrap(arg_wrap)
                .with_indent(IndentType::Continuation)
                .with_spacing(Spacing::one_space()),
            Block::leaf(")").with_spacing(Spacing::none()),
        ])
        .with_wrap(make_chain())
        .with_indent(IndentType::Continuation)
        .with_spacing(Spacing::none()),
    ]);

    let result = format_block(&root, 35);
    assert_eq!(result, "obj.first()\n        .method(longArg1, longArg2)");
}
