mod imports;
mod options;
mod xml;

#[cfg(test)]
mod tests;

pub use imports::{ImportGroup, default_import_layout, parse_import_layout};
pub use xml::{collect_unrecognized_options, parse_style_xml};

#[derive(Debug, Clone)]
pub struct StyleConfig {
    pub use_tabs: bool,
    pub smart_tabs: bool,
    pub indent_size: usize,
    pub continuation_indent: usize,
    pub right_margin: usize,

    pub brace_style: BraceStyle,
    pub class_brace_style: BraceStyle,
    pub method_brace_style: BraceStyle,

    pub keep_blank_lines_in_code: usize,
    pub keep_blank_lines_in_declarations: usize,
    pub keep_blank_lines_before_rbrace: usize,
    pub blank_lines_after_class_header: usize,
    pub blank_lines_around_field: usize,
    pub blank_lines_around_method: usize,
    pub blank_lines_around_initializer: usize,
    pub blank_lines_around_class: usize,
    pub blank_lines_around_field_in_interface: usize,
    pub blank_lines_around_method_in_interface: usize,
    pub blank_lines_after_anonymous_class_header: usize,
    pub blank_lines_before_imports: usize,
    pub blank_lines_after_imports: usize,

    pub if_brace_force: BraceForce,
    pub for_brace_force: BraceForce,
    pub while_brace_force: BraceForce,
    pub dowhile_brace_force: BraceForce,

    pub align_multiline_assignment: bool,
    pub align_multiline_ternary: bool,
    pub align_multiline_array_initializer: bool,
    pub align_group_field_declarations: bool,
    pub align_consecutive_variable_declarations: bool,
    pub align_subsequent_simple_methods: bool,
    pub align_multiline_parameters: bool,
    pub align_multiline_record_components: bool,
    pub align_multiline_for: bool,
    pub align_multiline_resources: bool,

    pub binary_operation_sign_on_next_line: bool,
    pub ternary_operation_signs_on_next_line: bool,
    pub ternary_wrap: WrapMode,
    pub binary_operation_wrap: WrapMode,

    pub keep_control_statement_in_one_line: bool,

    pub call_parameters_wrap: WrapMode,
    pub method_parameters_wrap: WrapMode,
    pub record_components_wrap: WrapMode,
    pub method_call_chain_wrap: WrapMode,
    pub indent_chained_calls: bool,
    pub prefer_parameters_wrap: bool,
    pub special_else_if_treatment: bool,
    pub throws_keyword_wrap: WrapMode,
    pub throws_list_wrap: WrapMode,
    pub extends_list_wrap: WrapMode,
    pub for_statement_wrap: WrapMode,
    pub array_initializer_wrap: WrapMode,
    pub assignment_wrap: WrapMode,
    pub space_before_array_initializer_lbrace: bool,

    pub space_before_if_parentheses: bool,
    pub space_before_for_parentheses: bool,
    pub space_before_while_parentheses: bool,
    pub space_before_switch_parentheses: bool,
    pub space_before_catch_parentheses: bool,
    pub space_before_synchronized_parentheses: bool,
    pub space_before_colon_in_foreach: bool,
    pub space_after_colon_in_foreach: bool,

    pub catch_on_new_line: bool,
    pub finally_on_new_line: bool,
    pub else_on_new_line: bool,
    pub wrap_comments: bool,
    pub keep_first_column_comment: bool,

    pub import_layout: Vec<ImportGroup>,
    pub class_count_to_use_import_on_demand: usize,
    pub names_count_to_use_import_on_demand: usize,

    pub class_annotation_wrap: WrapMode,
    pub field_annotation_wrap: WrapMode,
    pub method_annotation_wrap: WrapMode,
    pub parameter_annotation_wrap: WrapMode,
    pub align_multiline_annotation_parameters: bool,

    pub jd_align_param_comments: bool,
    pub jd_p_at_empty_lines: bool,
    pub jd_align_exception_comments: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BraceStyle {
    EndOfLine,
    NextLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BraceForce {
    DoNotForce,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapMode {
    DoNotWrap,
    WrapIfLong,
    ChopIfLong,
    WrapAlways,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            use_tabs: false,
            smart_tabs: false,
            indent_size: 4,
            continuation_indent: 8,
            right_margin: 120,
            brace_style: BraceStyle::EndOfLine,
            class_brace_style: BraceStyle::EndOfLine,
            method_brace_style: BraceStyle::EndOfLine,
            keep_blank_lines_in_code: 2,
            keep_blank_lines_in_declarations: 2,
            keep_blank_lines_before_rbrace: 2,
            blank_lines_after_class_header: 0,
            blank_lines_around_field: 0,
            blank_lines_around_method: 1,
            blank_lines_around_initializer: 1,
            blank_lines_around_class: 1,
            blank_lines_around_field_in_interface: 0,
            blank_lines_around_method_in_interface: 1,
            blank_lines_after_anonymous_class_header: 0,
            blank_lines_before_imports: 1,
            blank_lines_after_imports: 1,
            if_brace_force: BraceForce::DoNotForce,
            for_brace_force: BraceForce::DoNotForce,
            while_brace_force: BraceForce::DoNotForce,
            dowhile_brace_force: BraceForce::DoNotForce,
            align_multiline_assignment: false,
            align_multiline_ternary: false,
            align_multiline_array_initializer: false,
            align_group_field_declarations: false,
            align_consecutive_variable_declarations: false,
            align_subsequent_simple_methods: false,
            align_multiline_parameters: true,
            align_multiline_record_components: true,
            align_multiline_for: true,
            align_multiline_resources: true,
            binary_operation_sign_on_next_line: false,
            ternary_operation_signs_on_next_line: false,
            ternary_wrap: WrapMode::DoNotWrap,
            binary_operation_wrap: WrapMode::DoNotWrap,
            keep_control_statement_in_one_line: true,
            wrap_comments: false,
            keep_first_column_comment: true,
            call_parameters_wrap: WrapMode::DoNotWrap,
            method_parameters_wrap: WrapMode::DoNotWrap,
            record_components_wrap: WrapMode::WrapIfLong,
            method_call_chain_wrap: WrapMode::DoNotWrap,
            indent_chained_calls: true,
            prefer_parameters_wrap: false,
            special_else_if_treatment: true,
            throws_keyword_wrap: WrapMode::DoNotWrap,
            throws_list_wrap: WrapMode::DoNotWrap,
            extends_list_wrap: WrapMode::DoNotWrap,
            for_statement_wrap: WrapMode::DoNotWrap,
            array_initializer_wrap: WrapMode::DoNotWrap,
            assignment_wrap: WrapMode::DoNotWrap,
            space_before_array_initializer_lbrace: false,
            space_before_if_parentheses: true,
            space_before_for_parentheses: true,
            space_before_while_parentheses: true,
            space_before_switch_parentheses: true,
            space_before_catch_parentheses: true,
            space_before_synchronized_parentheses: true,
            space_before_colon_in_foreach: true,
            space_after_colon_in_foreach: true,
            catch_on_new_line: false,
            finally_on_new_line: false,
            else_on_new_line: false,
            import_layout: default_import_layout(),
            class_count_to_use_import_on_demand: 5,
            names_count_to_use_import_on_demand: 3,
            class_annotation_wrap: WrapMode::WrapAlways,
            field_annotation_wrap: WrapMode::WrapAlways,
            method_annotation_wrap: WrapMode::WrapAlways,
            parameter_annotation_wrap: WrapMode::DoNotWrap,
            align_multiline_annotation_parameters: false,
            jd_align_param_comments: true,
            jd_p_at_empty_lines: true,
            jd_align_exception_comments: true,
        }
    }
}
