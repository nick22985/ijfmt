use super::*;

struct ArgumentContext {
    enclosing_arg_wrap: Option<WrapId>,
    enclosing_cont_start_row: Option<usize>,
    in_binary_operand: bool,
    in_binary_rhs: bool,
    binary_rhs_operator_row: Option<usize>,
    binary_rhs_operator_on_new_line: bool,
    in_broken_ternary_branch: bool,
    in_same_line_outer_arg: bool,
    enclosing_arg_lparen_row: Option<usize>,
    arg_continuation_depth: u32,
    in_single_arg_list: bool,
    enclosing_arg_has_break: bool,
}

impl ArgumentContext {
    fn capture(builder: &BlockBuilder<'_>) -> Self {
        Self {
            enclosing_arg_wrap: builder.enclosing_arg_wrap,
            enclosing_cont_start_row: builder.enclosing_cont_start_row,
            in_binary_operand: builder.in_binary_operand,
            in_binary_rhs: builder.in_binary_rhs,
            binary_rhs_operator_row: builder.binary_rhs_operator_row,
            binary_rhs_operator_on_new_line: builder.binary_rhs_operator_on_new_line,
            in_broken_ternary_branch: builder.in_broken_ternary_branch,
            in_same_line_outer_arg: builder.in_same_line_outer_arg,
            enclosing_arg_lparen_row: builder.enclosing_arg_lparen_row,
            arg_continuation_depth: builder.arg_continuation_depth,
            in_single_arg_list: builder.in_single_arg_list,
            enclosing_arg_has_break: builder.enclosing_arg_has_break,
        }
    }

    fn restore(self, builder: &mut BlockBuilder<'_>) {
        builder.enclosing_arg_wrap = self.enclosing_arg_wrap;
        builder.enclosing_cont_start_row = self.enclosing_cont_start_row;
        builder.in_binary_operand = self.in_binary_operand;
        builder.in_binary_rhs = self.in_binary_rhs;
        builder.binary_rhs_operator_row = self.binary_rhs_operator_row;
        builder.binary_rhs_operator_on_new_line = self.binary_rhs_operator_on_new_line;
        builder.in_broken_ternary_branch = self.in_broken_ternary_branch;
        builder.in_same_line_outer_arg = self.in_same_line_outer_arg;
        builder.enclosing_arg_lparen_row = self.enclosing_arg_lparen_row;
        builder.arg_continuation_depth = self.arg_continuation_depth;
        builder.in_single_arg_list = self.in_single_arg_list;
        builder.enclosing_arg_has_break = self.enclosing_arg_has_break;
    }
}

impl BlockBuilder<'_> {
    pub(super) fn with_argument_context<T>(&mut self, build: impl FnOnce(&mut Self) -> T) -> T {
        let outer = ArgumentContext::capture(self);
        let result = build(self);
        outer.restore(self);
        result
    }
}
