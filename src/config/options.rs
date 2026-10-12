use super::{BraceForce, BraceStyle, StyleConfig, WrapMode};

#[derive(Clone, Copy)]
pub(super) enum OptionSection {
    Scheme,
    JavaGlobal,
    JavaIndentation,
    Java,
}

pub(super) fn apply_option(
    cfg: &mut StyleConfig,
    name: &str,
    value: &str,
    section: OptionSection,
) -> bool {
    match section {
        OptionSection::Scheme => {
            if name == "RIGHT_MARGIN" {
                cfg.right_margin = value.parse().unwrap_or(0);
            }
            true
        }
        OptionSection::JavaGlobal => apply_java_global_option(cfg, name, value),
        OptionSection::JavaIndentation => apply_indent_option(cfg, name, value),
        OptionSection::Java => apply_java_option(cfg, name, value),
    }
}

fn apply_java_global_option(cfg: &mut StyleConfig, name: &str, value: &str) -> bool {
    match name {
        "BLANK_LINES_AROUND_INITIALIZER" => {
            cfg.blank_lines_around_initializer = value.parse().unwrap_or(0)
        }
        "CLASS_COUNT_TO_USE_IMPORT_ON_DEMAND" => {
            cfg.class_count_to_use_import_on_demand = value.parse().unwrap_or(0)
        }
        "NAMES_COUNT_TO_USE_IMPORT_ON_DEMAND" => {
            cfg.names_count_to_use_import_on_demand = value.parse().unwrap_or(0)
        }
        "ALIGN_MULTILINE_ANNOTATION_PARAMETERS" => {
            cfg.align_multiline_annotation_parameters = value == "true"
        }
        "IMPORT_LAYOUT_TABLE" => {}
        "RECORD_COMPONENTS_WRAP" => {
            cfg.record_components_wrap = wrap_mode(value.parse().unwrap_or(0))
        }
        "JD_ALIGN_PARAM_COMMENTS" => cfg.jd_align_param_comments = value == "true",
        "JD_P_AT_EMPTY_LINES" => cfg.jd_p_at_empty_lines = value == "true",
        "JD_ALIGN_EXCEPTION_COMMENTS" => cfg.jd_align_exception_comments = value == "true",
        "INSERT_INNER_CLASS_IMPORTS" | "PACKAGES_TO_USE_IMPORT_ON_DEMAND" => {}
        _ => return false,
    }
    true
}

fn apply_indent_option(cfg: &mut StyleConfig, name: &str, value: &str) -> bool {
    match name {
        "USE_TAB_CHARACTER" => cfg.use_tabs = value == "true",
        "SMART_TABS" => cfg.smart_tabs = value == "true",
        "INDENT_SIZE" => cfg.indent_size = value.parse().unwrap_or(0),
        "TAB_SIZE" => cfg.indent_size = value.parse().unwrap_or(0),
        "CONTINUATION_INDENT_SIZE" => cfg.continuation_indent = value.parse().unwrap_or(0),
        _ => return false,
    }
    true
}

fn apply_java_option(cfg: &mut StyleConfig, name: &str, value: &str) -> bool {
    match name {
        "RIGHT_MARGIN" => cfg.right_margin = value.parse().unwrap_or(0),
        "BRACE_STYLE" => cfg.brace_style = brace_style(value.parse().unwrap_or(0)),
        "CLASS_BRACE_STYLE" => cfg.class_brace_style = brace_style(value.parse().unwrap_or(0)),
        "METHOD_BRACE_STYLE" => cfg.method_brace_style = brace_style(value.parse().unwrap_or(0)),

        "KEEP_BLANK_LINES_IN_CODE" => cfg.keep_blank_lines_in_code = value.parse().unwrap_or(0),
        "KEEP_BLANK_LINES_IN_DECLARATIONS" => {
            cfg.keep_blank_lines_in_declarations = value.parse().unwrap_or(0)
        }
        "BLANK_LINES_AFTER_CLASS_HEADER" => {
            cfg.blank_lines_after_class_header = value.parse().unwrap_or(0)
        }
        "KEEP_BLANK_LINES_BEFORE_RBRACE" => {
            cfg.keep_blank_lines_before_rbrace = value.parse().unwrap_or(0)
        }
        "BLANK_LINES_AROUND_FIELD" => cfg.blank_lines_around_field = value.parse().unwrap_or(0),
        "BLANK_LINES_AROUND_METHOD" => cfg.blank_lines_around_method = value.parse().unwrap_or(0),
        "BLANK_LINES_AROUND_CLASS" => cfg.blank_lines_around_class = value.parse().unwrap_or(0),
        "BLANK_LINES_AROUND_FIELD_IN_INTERFACE" => {
            cfg.blank_lines_around_field_in_interface = value.parse().unwrap_or(0)
        }
        "BLANK_LINES_AROUND_METHOD_IN_INTERFACE" => {
            cfg.blank_lines_around_method_in_interface = value.parse().unwrap_or(0)
        }
        "BLANK_LINES_AFTER_ANONYMOUS_CLASS_HEADER" => {
            cfg.blank_lines_after_anonymous_class_header = value.parse().unwrap_or(0)
        }
        "BLANK_LINES_BEFORE_IMPORTS" => cfg.blank_lines_before_imports = value.parse().unwrap_or(0),
        "BLANK_LINES_AFTER_IMPORTS" => cfg.blank_lines_after_imports = value.parse().unwrap_or(0),
        "CODE_STYLE_DEFAULTS" => {
            if value == "KOTLIN_OFFICIAL" {
                cfg.keep_blank_lines_in_code = 1;
            }
        }

        "IF_BRACE_FORCE" => cfg.if_brace_force = brace_force(value.parse().unwrap_or(0)),
        "FOR_BRACE_FORCE" => cfg.for_brace_force = brace_force(value.parse().unwrap_or(0)),
        "WHILE_BRACE_FORCE" => cfg.while_brace_force = brace_force(value.parse().unwrap_or(0)),
        "DOWHILE_BRACE_FORCE" => cfg.dowhile_brace_force = brace_force(value.parse().unwrap_or(0)),

        "ALIGN_MULTILINE_ASSIGNMENT" => cfg.align_multiline_assignment = value == "true",
        "ALIGN_MULTILINE_TERNARY_OPERATION" => cfg.align_multiline_ternary = value == "true",
        "ALIGN_MULTILINE_ARRAY_INITIALIZER_EXPRESSION" => {
            cfg.align_multiline_array_initializer = value == "true"
        }
        "ALIGN_GROUP_FIELD_DECLARATIONS" => cfg.align_group_field_declarations = value == "true",
        "ALIGN_CONSECUTIVE_VARIABLE_DECLARATIONS" => {
            cfg.align_consecutive_variable_declarations = value == "true"
        }
        "ALIGN_SUBSEQUENT_SIMPLE_METHODS" => cfg.align_subsequent_simple_methods = value == "true",
        "ALIGN_MULTILINE_PARAMETERS" => cfg.align_multiline_parameters = value == "true",
        "ALIGN_MULTILINE_RECORD_COMPONENTS" => {
            cfg.align_multiline_record_components = value == "true"
        }
        "ALIGN_MULTILINE_FOR" => cfg.align_multiline_for = value == "true",

        "BINARY_OPERATION_SIGN_ON_NEXT_LINE" => {
            cfg.binary_operation_sign_on_next_line = value == "true"
        }
        "TERNARY_OPERATION_SIGNS_ON_NEXT_LINE" => {
            cfg.ternary_operation_signs_on_next_line = value == "true"
        }
        "TERNARY_OPERATION_WRAP" => cfg.ternary_wrap = wrap_mode(value.parse().unwrap_or(0)),

        "KEEP_CONTROL_STATEMENT_IN_ONE_LINE" => {
            cfg.keep_control_statement_in_one_line = value == "true"
        }
        "WRAP_COMMENTS" => cfg.wrap_comments = value == "true",
        "KEEP_FIRST_COLUMN_COMMENT" => cfg.keep_first_column_comment = value == "true",
        "CALL_PARAMETERS_WRAP" => cfg.call_parameters_wrap = wrap_mode(value.parse().unwrap_or(0)),
        "METHOD_PARAMETERS_WRAP" => {
            cfg.method_parameters_wrap = wrap_mode(value.parse().unwrap_or(0))
        }
        "CATCH_ON_NEW_LINE" => cfg.catch_on_new_line = value == "true",
        "FINALLY_ON_NEW_LINE" => cfg.finally_on_new_line = value == "true",
        "ELSE_ON_NEW_LINE" => cfg.else_on_new_line = value == "true",

        "CLASS_ANNOTATION_WRAP" => {
            cfg.class_annotation_wrap = wrap_mode(value.parse().unwrap_or(0))
        }
        "FIELD_ANNOTATION_WRAP" => {
            cfg.field_annotation_wrap = wrap_mode(value.parse().unwrap_or(0))
        }
        "METHOD_ANNOTATION_WRAP" => {
            cfg.method_annotation_wrap = wrap_mode(value.parse().unwrap_or(0))
        }
        "PARAMETER_ANNOTATION_WRAP" => {
            cfg.parameter_annotation_wrap = wrap_mode(value.parse().unwrap_or(0))
        }

        "BINARY_OPERATION_WRAP" => {
            cfg.binary_operation_wrap = wrap_mode(value.parse().unwrap_or(0))
        }
        "METHOD_CALL_CHAIN_WRAP" => {
            cfg.method_call_chain_wrap = wrap_mode(value.parse().unwrap_or(0))
        }
        "INDENT_CHAINED_CALLS" => cfg.indent_chained_calls = value == "true",
        "PREFER_PARAMETERS_WRAP" => cfg.prefer_parameters_wrap = value == "true",
        "SPECIAL_ELSE_IF_TREATMENT" => cfg.special_else_if_treatment = value == "true",
        "THROWS_KEYWORD_WRAP" => cfg.throws_keyword_wrap = wrap_mode(value.parse().unwrap_or(0)),
        "THROWS_LIST_WRAP" => cfg.throws_list_wrap = wrap_mode(value.parse().unwrap_or(0)),
        "EXTENDS_LIST_WRAP" => cfg.extends_list_wrap = wrap_mode(value.parse().unwrap_or(0)),
        "FOR_STATEMENT_WRAP" => cfg.for_statement_wrap = wrap_mode(value.parse().unwrap_or(0)),
        "ARRAY_INITIALIZER_WRAP" => {
            cfg.array_initializer_wrap = wrap_mode(value.parse().unwrap_or(0))
        }
        "ASSIGNMENT_WRAP" => cfg.assignment_wrap = wrap_mode(value.parse().unwrap_or(0)),
        "SPACE_BEFORE_ARRAY_INITIALIZER_LBRACE" => {
            cfg.space_before_array_initializer_lbrace = value == "true"
        }
        "SPACE_BEFORE_IF_PARENTHESES" => cfg.space_before_if_parentheses = value == "true",
        "SPACE_BEFORE_FOR_PARENTHESES" => cfg.space_before_for_parentheses = value == "true",
        "SPACE_BEFORE_WHILE_PARENTHESES" => cfg.space_before_while_parentheses = value == "true",
        "SPACE_BEFORE_SWITCH_PARENTHESES" => cfg.space_before_switch_parentheses = value == "true",
        "SPACE_BEFORE_CATCH_PARENTHESES" => cfg.space_before_catch_parentheses = value == "true",
        "SPACE_BEFORE_SYNCHRONIZED_PARENTHESES" => {
            cfg.space_before_synchronized_parentheses = value == "true"
        }
        "SPACE_BEFORE_COLON_IN_FOREACH" => cfg.space_before_colon_in_foreach = value == "true",
        "SPACE_AFTER_COLON_IN_FOREACH" => cfg.space_after_colon_in_foreach = value == "true",

        "ALIGN_MULTILINE_RESOURCES" => cfg.align_multiline_resources = value == "true",

        "PARENT_SETTINGS_INSTALLED" | "WRAP_ON_TYPING" => {}

        _ => return false,
    }
    true
}

fn brace_style(v: i32) -> BraceStyle {
    if v == 2 {
        BraceStyle::NextLine
    } else {
        BraceStyle::EndOfLine
    }
}

fn brace_force(v: i32) -> BraceForce {
    if v >= 3 {
        BraceForce::Always
    } else {
        BraceForce::DoNotForce
    }
}

fn wrap_mode(v: i32) -> WrapMode {
    match v {
        0 => WrapMode::DoNotWrap,
        1 => WrapMode::WrapIfLong,
        2 => WrapMode::ChopIfLong,
        _ => WrapMode::WrapAlways,
    }
}
