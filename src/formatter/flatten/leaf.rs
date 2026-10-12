use super::super::ir::*;

#[derive(Debug)]
pub struct LeafEntry {
    pub text: String,
    pub wraps: Vec<WrapId>,
    pub white_space: WhiteSpace,
    pub indent_stack: Vec<IndentType>,
    pub alignment: Option<AlignmentId>,
    pub source_offset: Option<usize>,
    pub source_has_line_break_before: bool,
    pub raw_source: bool,
    pub jd_regenerated: bool,
    pub extra_width: usize,
    pub pre_run_width: usize,
    pub line_comment: bool,
    pub statement_comment: bool,
    pub member_bound_comment: bool,
    pub anchor: Option<Anchor>,
    pub anchor_self_exempt: bool,
    pub paren_close: bool,
    pub arg_probe_for: Option<usize>,
    pub chain_probe_for: Option<usize>,
    pub chain_dot: Option<(usize, u8, Option<usize>)>,
    pub bin_op: Option<(usize, Option<usize>, Option<usize>)>,
    pub tern_colon: Option<(usize, Option<usize>)>,
    pub tern_nest: Option<(usize, bool)>,
    pub tern_nest_col: Option<usize>,
    pub tern_signcol: bool,
    pub tern_self: Option<usize>,
    pub tern_branch: Option<usize>,
    pub tern_cmt: Option<usize>,
    pub tern_bin: Option<usize>,
    pub bin_first: Option<usize>,
    pub arr_first: Option<usize>,
    pub arr_close: bool,
    pub arr_elem: Option<usize>,
    pub param_first: Option<usize>,
    pub span_end: usize,
    pub break_bonus: Option<(usize, Option<usize>)>,
    pub lambda_body_cont: Option<usize>,
    pub paren_cwf: Option<usize>,
    pub res_align: bool,
    pub align_scope: Option<(AlignmentId, usize)>,
    pub align_chain: Vec<AlignmentId>,
    pub encl_chain: Vec<usize>,
}

#[derive(Debug, Clone)]
pub enum Anchor {
    Body(
        std::rc::Rc<Vec<usize>>,
        usize,
        Option<std::rc::Rc<ArgCtx>>,
        usize,
    ),
    Arg(std::rc::Rc<ArgCtx>),
}

#[derive(Debug)]
pub struct ArgCtx {
    pub lparen: usize,
    pub elem_has_cont: bool,
    pub channel: bool,
    pub chain_channel: bool,
    pub chain_wrapper_static: bool,
    pub chain_line_capped: bool,
    pub seg_cancel: bool,
    pub collapsed: bool,
    pub enum_const: bool,
    pub bin_suppressed: bool,
    pub smart: bool,
    pub ignore_parent_wraps: bool,
    pub chain: Option<usize>,
    pub seg_first: Option<usize>,
    pub parent: Option<std::rc::Rc<ArgCtx>>,
    pub body: Option<(std::rc::Rc<Vec<usize>>, usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct WhiteSpace {
    pub line_feeds: usize,
    pub spaces: usize,
}

impl WhiteSpace {
    pub fn new() -> Self {
        WhiteSpace {
            line_feeds: 0,
            spaces: 0,
        }
    }

    pub fn contains_line_feeds(&self) -> bool {
        self.line_feeds > 0
    }

    pub fn ensure_line_feed(&mut self) {
        if self.line_feeds == 0 {
            self.line_feeds = 1;
        }
    }

    pub fn remove_line_feeds(&mut self) {
        self.line_feeds = 0;
    }
}
