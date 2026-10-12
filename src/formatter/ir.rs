use std::fmt;

pub(super) fn is_binary_operator_text(text: &str) -> bool {
    matches!(
        text,
        "+" | "-"
            | "*"
            | "/"
            | "%"
            | "&&"
            | "||"
            | "&"
            | "|"
            | "^"
            | "<<"
            | ">>"
            | ">>>"
            | "=="
            | "!="
            | "<"
            | ">"
            | "<="
            | ">="
            | "instanceof"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WrapId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapType {
    None,
    Normal,
    ChopIfNeeded,
    Always,
}

#[derive(Debug, Clone)]
pub struct Wrap {
    pub id: WrapId,
    pub wrap_type: WrapType,
    pub wrap_first_element: bool,
    pub parent: Option<WrapId>,
    pub ignore_parent_wraps: bool,
    pub cand_ignore_parents: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentType {
    None,
    Normal,
    Continuation,
    Spaces(usize),
    BlockReset,
    LambdaParamGlued,
    CancelOneContinuation,
    CancelOneNormal,
    BlockResetKeepOne,
    ColumnZero,
    NormalIfWrapped(usize),
    ContinuationIfWrapped(usize),
}

#[derive(Debug, Clone)]
pub struct Spacing {
    pub min_spaces: u32,
    pub max_spaces: u32,
    pub min_line_feeds: u32,
    pub keep_blank_lines: u32,
    pub keep_line_breaks: bool,
}

impl Spacing {
    pub fn one_space() -> Self {
        Spacing {
            min_spaces: 1,
            max_spaces: 1,
            min_line_feeds: 0,
            keep_blank_lines: 0,
            keep_line_breaks: false,
        }
    }

    pub fn none() -> Self {
        Spacing {
            min_spaces: 0,
            max_spaces: 0,
            min_line_feeds: 0,
            keep_blank_lines: 0,
            keep_line_breaks: false,
        }
    }

    pub fn new_line() -> Self {
        Spacing {
            min_spaces: 0,
            max_spaces: 0,
            min_line_feeds: 1,
            keep_blank_lines: 0,
            keep_line_breaks: false,
        }
    }

    pub fn blank_line() -> Self {
        Spacing {
            min_spaces: 0,
            max_spaces: 0,
            min_line_feeds: 2,
            keep_blank_lines: 0,
            keep_line_breaks: false,
        }
    }

    pub fn spaces(min: u32, max: u32) -> Self {
        Spacing {
            min_spaces: min,
            max_spaces: max,
            min_line_feeds: 0,
            keep_blank_lines: 0,
            keep_line_breaks: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlignmentId(pub u32);

#[derive(Debug, Clone, Copy)]
pub struct ArgAnchorInfo {
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
}

#[derive(Debug)]
pub struct Block {
    pub text: String,
    pub children: Vec<Block>,
    pub wrap: Option<Wrap>,
    pub indent: IndentType,
    pub spacing: Option<Spacing>,
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
    pub anchor_start: bool,
    pub anchor_body: bool,
    pub arg_list_anchor: Option<ArgAnchorInfo>,
    pub chain_anchor: bool,
    pub chain_probe: bool,
    pub chain_seg_channel: u8,
    pub arg_probe: bool,
    pub anchor_exempt: bool,
    pub bin_anchor: bool,
    pub bin_probe: bool,
    pub bin_paren_anchor: bool,
    pub bin_scope: bool,
    pub wrap_no_promote: bool,
    pub tern_colon_probe: bool,
    pub tern_scope: bool,
    pub break_bonus: bool,
    pub break_bonus_gate: bool,
    pub lambda_body_cont: bool,
    pub paren_cwf_probe: bool,
    pub paren_close_probe: bool,
    pub res_align: bool,
    pub align_barrier: bool,
    pub tern_nest_probe: bool,
    pub tern_nest_col_probe: bool,
    pub tern_nest_deep_probe: bool,
    pub tern_signcol_probe: bool,
    pub tern_self_scope: bool,
    pub tern_self_probe: bool,
    pub tern_branch_probe: bool,
    pub tern_cmt_scope: bool,
    pub tern_cmt_probe: bool,
    pub tern_bin_probe: bool,
    pub bin_child_probe: bool,
    pub arr_scope: bool,
    pub arr_probe: bool,
    pub arr_close: bool,
    pub param_scope: bool,
    pub param_probe: bool,
}

impl Block {
    pub fn with_line_comment(mut self) -> Self {
        self.line_comment = true;
        self
    }

    pub fn leaf(text: impl Into<String>) -> Self {
        Self::with_contents(text.into(), Vec::new())
    }

    pub fn node(children: Vec<Block>) -> Self {
        Self::with_contents(String::new(), children)
    }

    fn with_contents(text: String, children: Vec<Block>) -> Self {
        Block {
            text,
            children,
            wrap: None,
            indent: IndentType::None,
            spacing: None,
            alignment: None,
            source_offset: None,
            source_has_line_break_before: false,
            raw_source: false,
            jd_regenerated: false,
            extra_width: 0,
            pre_run_width: 0,
            line_comment: false,
            statement_comment: false,
            member_bound_comment: false,
            anchor_start: false,
            anchor_body: false,
            arg_list_anchor: None,
            chain_anchor: false,
            chain_probe: false,
            chain_seg_channel: 0,
            arg_probe: false,
            anchor_exempt: false,
            bin_anchor: false,
            bin_probe: false,
            bin_paren_anchor: false,
            bin_scope: false,
            wrap_no_promote: false,
            tern_colon_probe: false,
            tern_scope: false,
            break_bonus: false,
            break_bonus_gate: false,
            lambda_body_cont: false,
            paren_cwf_probe: false,
            paren_close_probe: false,
            res_align: false,
            align_barrier: false,
            tern_nest_probe: false,
            tern_nest_col_probe: false,
            tern_nest_deep_probe: false,
            tern_signcol_probe: false,
            tern_cmt_scope: false,
            tern_self_scope: false,
            tern_self_probe: false,
            tern_branch_probe: false,
            tern_cmt_probe: false,
            tern_bin_probe: false,
            bin_child_probe: false,
            arr_scope: false,
            arr_probe: false,
            arr_close: false,
            param_scope: false,
            param_probe: false,
        }
    }

    pub fn with_wrap(mut self, wrap: Wrap) -> Self {
        self.wrap = Some(wrap);
        self
    }

    pub fn with_wrap_if(self, cond: bool, wrap: Wrap) -> Self {
        if cond { self.with_wrap(wrap) } else { self }
    }

    pub fn with_indent(mut self, indent: IndentType) -> Self {
        self.indent = indent;
        self
    }

    pub fn with_spacing(mut self, spacing: Spacing) -> Self {
        self.spacing = Some(spacing);
        self
    }

    pub fn with_alignment(mut self, id: AlignmentId) -> Self {
        self.alignment = Some(id);
        self
    }

    pub fn with_source_offset(mut self, offset: usize) -> Self {
        self.source_offset = Some(offset);
        self
    }

    pub fn with_source_line_break(mut self) -> Self {
        self.source_has_line_break_before = true;
        if let Some(first) = self.children.first_mut() {
            let taken = std::mem::replace(first, Block::leaf(""));
            *first = taken.with_source_line_break();
        }
        self
    }

    pub fn with_jd_regenerated(mut self) -> Self {
        self.jd_regenerated = true;
        self
    }

    pub fn with_raw_source(mut self) -> Self {
        self.raw_source = true;
        self
    }

    pub fn with_extra_width(mut self, n: usize) -> Self {
        self.extra_width = n;
        self
    }

    pub fn with_pre_run_width(mut self, n: usize) -> Self {
        self.pre_run_width = n;
        self
    }

    pub fn with_statement_comment(mut self) -> Self {
        self.statement_comment = true;
        self
    }

    pub fn with_member_bound_comment(mut self) -> Self {
        self.member_bound_comment = true;
        self
    }

    pub fn with_anchor_start(mut self) -> Self {
        self.anchor_start = true;
        self
    }

    pub fn with_anchor_body(mut self) -> Self {
        self.anchor_body = true;
        self
    }

    pub fn with_arg_list_anchor(mut self, info: ArgAnchorInfo) -> Self {
        self.arg_list_anchor = Some(info);
        self
    }

    pub fn with_chain_anchor(mut self) -> Self {
        self.chain_anchor = true;
        self
    }

    pub fn with_chain_probe(mut self) -> Self {
        self.chain_probe = true;
        self
    }

    pub fn with_chain_seg_channel(mut self, channel: u8) -> Self {
        self.chain_seg_channel = channel;
        self
    }

    pub fn with_arg_probe(mut self) -> Self {
        self.arg_probe = true;
        self
    }

    pub fn with_anchor_exempt(mut self) -> Self {
        self.anchor_exempt = true;
        self
    }

    pub fn with_bin_anchor(mut self) -> Self {
        self.bin_anchor = true;
        self
    }

    pub fn with_bin_scope(mut self) -> Self {
        self.bin_scope = true;
        self
    }

    pub fn with_wrap_no_promote(mut self) -> Self {
        self.wrap_no_promote = true;
        self
    }

    pub fn with_break_bonus(mut self) -> Self {
        self.break_bonus = true;
        self
    }

    pub fn with_gated_break_bonus(mut self) -> Self {
        self.break_bonus = true;
        self.break_bonus_gate = true;
        self
    }

    pub fn with_lambda_body_cont(mut self) -> Self {
        self.lambda_body_cont = true;
        self
    }

    pub fn with_paren_cwf_probe(mut self) -> Self {
        self.paren_cwf_probe = true;
        self
    }

    pub fn with_paren_close_probe(mut self) -> Self {
        self.paren_close_probe = true;
        self
    }

    pub fn with_align_barrier(mut self) -> Self {
        self.align_barrier = true;
        self
    }

    pub fn with_res_align(mut self) -> Self {
        self.res_align = true;
        self
    }

    pub fn with_tern_scope(mut self) -> Self {
        self.tern_scope = true;
        self
    }

    pub fn with_tern_nest_probe(mut self) -> Self {
        self.tern_nest_probe = true;
        self
    }

    pub fn with_tern_nest_col_probe(mut self) -> Self {
        self.tern_nest_col_probe = true;
        self
    }

    pub fn with_tern_nest_deep_probe(mut self) -> Self {
        self.tern_nest_deep_probe = true;
        self
    }

    pub fn with_tern_signcol_probe(mut self) -> Self {
        self.tern_signcol_probe = true;
        self
    }

    pub fn with_tern_self_scope(mut self) -> Self {
        self.tern_self_scope = true;
        self
    }

    pub fn with_tern_branch_probe(mut self) -> Self {
        self.tern_branch_probe = true;
        self
    }

    pub fn with_tern_self_probe(mut self) -> Self {
        self.tern_self_probe = true;
        self
    }

    pub fn with_tern_cmt_scope(mut self) -> Self {
        self.tern_cmt_scope = true;
        self
    }

    pub fn with_tern_cmt_probe(mut self) -> Self {
        self.tern_cmt_probe = true;
        self
    }

    pub fn with_tern_bin_probe(mut self) -> Self {
        self.tern_bin_probe = true;
        self
    }

    pub fn with_arr_scope(mut self) -> Self {
        self.arr_scope = true;
        self
    }

    pub fn with_arr_probe(mut self) -> Self {
        self.arr_probe = true;
        self
    }

    pub fn with_param_scope(mut self) -> Self {
        self.param_scope = true;
        self
    }

    pub fn with_param_probe(mut self) -> Self {
        self.param_probe = true;
        self
    }

    pub fn with_arr_close(mut self) -> Self {
        self.arr_probe = true;
        self.arr_close = true;
        self
    }

    pub fn with_bin_child_probe(mut self) -> Self {
        self.bin_child_probe = true;
        self
    }

    pub fn with_tern_colon_probe(mut self) -> Self {
        self.tern_colon_probe = true;
        self
    }

    pub fn with_bin_paren_anchor(mut self) -> Self {
        self.bin_paren_anchor = true;
        self
    }

    pub fn with_bin_probe(mut self) -> Self {
        self.bin_probe = true;
        self
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_leaf() {
            write!(f, "Leaf({:?})", self.text)
        } else {
            write!(f, "Node({} children)", self.children.len())
        }
    }
}

pub struct IdGen {
    next_wrap: u32,
    next_alignment: u32,
}

impl IdGen {
    pub fn new() -> Self {
        IdGen {
            next_wrap: 0,
            next_alignment: 0,
        }
    }

    pub fn wrap_id(&mut self) -> WrapId {
        let id = WrapId(self.next_wrap);
        self.next_wrap += 1;
        id
    }

    pub fn alignment_id(&mut self) -> AlignmentId {
        let id = AlignmentId(self.next_alignment);
        self.next_alignment += 1;
        id
    }

    pub fn wrap(&mut self, wrap_type: WrapType) -> Wrap {
        Wrap {
            id: self.wrap_id(),
            wrap_type,
            wrap_first_element: false,
            parent: None,
            ignore_parent_wraps: false,
            cand_ignore_parents: false,
        }
    }

    #[cfg(test)]
    pub fn child_wrap(&mut self, parent: WrapId, wrap_type: WrapType) -> Wrap {
        Wrap {
            id: self.wrap_id(),
            wrap_type,
            wrap_first_element: false,
            parent: Some(parent),
            ignore_parent_wraps: false,
            cand_ignore_parents: false,
        }
    }
}
