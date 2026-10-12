use tree_sitter::Node;

pub(super) struct TernarySource<'tree> {
    pub(super) cond: Option<Node<'tree>>,
    pub(super) consequence: Option<Node<'tree>>,
    pub(super) alternative: Option<Node<'tree>>,
    pub(super) q_row: Option<usize>,
    pub(super) colon_row: Option<usize>,
    pub(super) extras_before_q: Vec<Node<'tree>>,
    pub(super) extras_after_cons: Vec<Node<'tree>>,
    pub(super) extras_after_colon: Vec<Node<'tree>>,
    pub(super) extras_after_alt: Vec<Node<'tree>>,
    pub(super) cons_start_row: usize,
    pub(super) alt_start_row: usize,
    pub(super) q_on_new_line: bool,
    pub(super) colon_on_new_line: bool,
    pub(super) q_at_end_of_line: bool,
    pub(super) colon_at_end_of_line: bool,
    pub(super) source_multiline: bool,
}

impl<'tree> TernarySource<'tree> {
    pub(super) fn scan(
        node: Node<'tree>,
        cond: Option<Node<'tree>>,
        consequence: Option<Node<'tree>>,
        alternative: Option<Node<'tree>>,
        source_multiline: bool,
    ) -> Self {
        let mut q_row = None;
        let mut colon_row = None;
        let mut extras_before_q: Vec<Node> = Vec::new();
        let mut extras_after_cons: Vec<Node> = Vec::new();
        let mut extras_after_colon: Vec<Node> = Vec::new();
        let mut extras_after_alt: Vec<Node> = Vec::new();
        {
            let cond_end = cond.map(|c| c.end_byte()).unwrap_or(0);
            let cons_end = consequence.map(|c| c.end_byte()).unwrap_or(0);
            let alt_start = alternative.map(|c| c.start_byte()).unwrap_or(0);
            let alt_end = alternative.map(|c| c.end_byte()).unwrap_or(0);
            let mut tc = node.walk();
            for child in node.children(&mut tc) {
                if child.kind() == "?" {
                    q_row = Some(child.start_position().row);
                } else if child.kind() == ":" {
                    colon_row = Some(child.start_position().row);
                } else if child.is_extra() && child.start_byte() >= cond_end && q_row.is_none() {
                    extras_before_q.push(child);
                } else if child.is_extra()
                    && child.start_byte() >= cons_end
                    && colon_row.is_none()
                    && q_row.is_some()
                {
                    extras_after_cons.push(child);
                } else if child.is_extra() && colon_row.is_some() && child.start_byte() < alt_start
                {
                    extras_after_colon.push(child);
                } else if child.is_extra() && child.start_byte() >= alt_end && colon_row.is_some() {
                    extras_after_alt.push(child);
                }
            }
        }
        let cond_end_row = cond.map(|c| c.end_position().row).unwrap_or(0);
        let cons_end_row = consequence.map(|c| c.end_position().row).unwrap_or(0);
        let cons_start_row = consequence.map(|c| c.start_position().row).unwrap_or(0);
        let alt_start_row = alternative.map(|c| c.start_position().row).unwrap_or(0);
        let q_on_new_line = source_multiline && q_row.is_some_and(|r| r > cond_end_row);
        let colon_on_new_line = source_multiline && colon_row.is_some_and(|r| r > cons_end_row);

        let q_at_end_of_line =
            source_multiline && (q_row == Some(cond_end_row)) && cons_start_row > cond_end_row;
        let colon_at_end_of_line =
            source_multiline && (colon_row == Some(cons_end_row)) && alt_start_row > cons_end_row;

        Self {
            cond,
            consequence,
            alternative,
            q_row,
            colon_row,
            extras_before_q,
            extras_after_cons,
            extras_after_colon,
            extras_after_alt,
            cons_start_row,
            alt_start_row,
            q_on_new_line,
            colon_on_new_line,
            q_at_end_of_line,
            colon_at_end_of_line,
            source_multiline,
        }
    }
}
