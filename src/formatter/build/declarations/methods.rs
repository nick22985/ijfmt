use crate::config::WrapMode;
use tree_sitter::Node;

use crate::formatter::build::util::wrap_type_from_mode;
use crate::formatter::build::{BlockBuilder, MalformedSupertypeRecovery};
use crate::formatter::ir::*;

impl<'a> BlockBuilder<'a> {
    fn member_binds_preceding_comment(&self, node: Node) -> bool {
        let mut sib = node.prev_sibling();
        while let Some(c) = sib {
            if !matches!(c.kind(), "line_comment" | "block_comment") {
                break;
            }
            if self.is_doc_comment_node(c) {
                return true;
            }
            sib = c.prev_sibling();
        }
        let mut next_start = node.start_byte();
        let mut sib = node.prev_sibling();
        let mut bound = false;
        while let Some(c) = sib {
            if !matches!(c.kind(), "line_comment" | "block_comment") {
                break;
            }
            let gap = &self.src[c.end_byte()..next_start.min(self.src.len())];
            if gap.iter().filter(|&&b| b == b'\n').count() > 1 {
                break;
            }
            let starts_line = self.src[..c.start_byte()]
                .iter()
                .rev()
                .take_while(|&&b| b != b'\n')
                .all(|&b| b == b' ' || b == b'\t')
                && c.start_byte() > 0;
            if starts_line {
                bound = true;
            }
            next_start = c.start_byte();
            sib = c.prev_sibling();
        }
        bound
    }

    pub(crate) fn method_decl(&mut self, node: Node) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_child: Option<Node> = None;
        let mut prev_was_annotated_modifiers = false;
        let malformed_signature = self.malformed_supertype_recovery
            == Some(MalformedSupertypeRecovery::Generic)
            || (self.malformed_supertype_recovery == Some(MalformedSupertypeRecovery::Scoped)
                && node.kind() == "method_declaration");

        let throws_wrap_type = wrap_type_from_mode(self.cfg.throws_keyword_wrap);
        let throws_list_wrap_type = wrap_type_from_mode(self.cfg.throws_list_wrap);
        let outer_throws_wrap_id = if throws_wrap_type != WrapType::None {
            Some(self.ids.wrap_id())
        } else {
            None
        };

        let all_children: Vec<_> = node.children(&mut cursor).collect();
        let mut composed = self.member_binds_preceding_comment(node);
        let mut prev_was_comment = composed;
        for child in all_children.iter().copied() {
            if child.is_extra() {
                let block = self.comment_leaf(child);
                let block = if !composed && prev_end_byte.is_some() {
                    let mut part_of_block = false;
                    for n in all_children.iter() {
                        if n.start_byte() < child.end_byte() {
                            continue;
                        }
                        if n.kind() == "line_comment" {
                            continue;
                        }
                        part_of_block = matches!(n.kind(), "block" | "constructor_body");
                        break;
                    }
                    if part_of_block {
                        block
                    } else {
                        block.with_indent(IndentType::Normal)
                    }
                } else {
                    block
                };
                let spacing = prev_end_byte.map(|pb| {
                    if prev_was_annotated_modifiers
                        && self.cfg.method_annotation_wrap != WrapMode::DoNotWrap
                    {
                        Spacing::new_line()
                    } else {
                        let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                        if gap.contains(&b'\n') {
                            Spacing::new_line()
                        } else {
                            Spacing::one_space()
                        }
                    }
                });
                let block = match spacing {
                    Some(s) => block.with_spacing(s),
                    None => block,
                };
                children.push(block);
                prev_end_byte = Some(child.end_byte());
                prev_was_comment = true;
                continue;
            }

            if prev_was_comment {
                composed = true;
            }
            prev_was_comment = false;

            let throws_has_outer_cont = if child.kind() == "throws"
                && let Some(pb) = prev_end_byte
            {
                if throws_wrap_type != WrapType::None {
                    true
                } else {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    gap.contains(&b'\n')
                }
            } else {
                false
            };

            let malformed_body_break = malformed_signature
                && matches!(child.kind(), "block" | "constructor_body")
                && self.cfg.right_margin > 0
                && child.start_position().column >= self.cfg.right_margin;
            let block = match child.kind() {
                "formal_parameters" => self.formal_parameters(child),
                "throws" => self.throws_clause(
                    child,
                    throws_list_wrap_type,
                    outer_throws_wrap_id,
                    throws_has_outer_cont,
                ),
                "block" | "constructor_body" => {
                    let block = self.code_block(child);
                    if self.malformed_supertype_recovery
                        == Some(MalformedSupertypeRecovery::Generic)
                        && !malformed_body_break
                    {
                        block.with_indent(IndentType::CancelOneNormal)
                    } else {
                        block
                    }
                }
                _ => self.node(child),
            };

            let is_method_body = matches!(child.kind(), "block" | "constructor_body");

            let spacing = if let Some(pb) = prev_end_byte {
                if child.kind() == "formal_parameters" {
                    if malformed_signature && node.kind() == "method_declaration" {
                        Spacing::one_space()
                    } else {
                        Spacing::none()
                    }
                } else if is_method_body {
                    if malformed_signature {
                        if malformed_body_break {
                            Spacing::new_line()
                        } else if prev_child.is_some_and(|previous| {
                            previous.kind() == "formal_parameters"
                                && previous.named_child_count() == 0
                        }) || prev_child
                            .is_some_and(|previous| previous.kind() == "throws")
                        {
                            Spacing::one_space()
                        } else {
                            Spacing::none()
                        }
                    } else {
                        use crate::config::BraceStyle;
                        match self.cfg.method_brace_style {
                            BraceStyle::NextLine => self.lbrace_new_line(child),
                            BraceStyle::EndOfLine => Spacing::one_space(),
                        }
                    }
                } else if prev_was_annotated_modifiers
                    && self.cfg.method_annotation_wrap != WrapMode::DoNotWrap
                {
                    Spacing::new_line()
                } else if child.kind() == "throws" {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.contains(&b'\n') {
                        Spacing::new_line()
                    } else {
                        Spacing::one_space()
                    }
                } else {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.contains(&b'\n') {
                        Spacing::new_line()
                    } else if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                        Spacing::one_space()
                    } else {
                        Spacing::none()
                    }
                }
            } else {
                Spacing::none()
            };

            let block = if child.kind() == "throws"
                && let Some(pb) = prev_end_byte
            {
                if throws_wrap_type != WrapType::None {
                    let wrap_id = outer_throws_wrap_id.unwrap();
                    let w = Wrap {
                        id: wrap_id,
                        wrap_type: throws_wrap_type,
                        wrap_first_element: true,
                        parent: None,
                        ignore_parent_wraps: false,
                        cand_ignore_parents: false,
                    };
                    self.wraps.insert(w.id, w.clone());
                    block.with_wrap(w).with_indent(IndentType::Continuation)
                } else {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.contains(&b'\n') {
                        block.with_indent(IndentType::Continuation)
                    } else {
                        block
                    }
                }
            } else {
                block
            };

            let block = if prev_end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };

            prev_was_annotated_modifiers = child.kind() == "modifiers" && {
                let mut mc = child.walk();
                let kids: Vec<_> = child.children(&mut mc).filter(|c| !c.is_extra()).collect();
                let last = kids.last();
                match last {
                    Some(last) if matches!(last.kind(), "marker_annotation" | "annotation") => {
                        let has_keyword_before = kids.iter().take(kids.len() - 1).any(|c| {
                            !matches!(c.kind(), "marker_annotation" | "annotation")
                                && !c.is_extra()
                                && c.child_count() == 0
                        });
                        if has_keyword_before {
                            false
                        } else if last.kind() == "marker_annotation" {
                            !self.is_type_use_annotation(*last)
                        } else {
                            true
                        }
                    }
                    _ => false,
                }
            };

            prev_end_byte = Some(child.end_byte());
            prev_child = Some(child);
            children.push(block);
        }

        Block::node(children)
    }

    fn throws_clause(
        &mut self,
        node: Node,
        wrap_type: WrapType,
        outer_wrap_id: Option<WrapId>,
        has_outer_cont: bool,
    ) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let wrap_id = self.ids.wrap_id();
        let mut type_index = 0;
        let mut prev_end_byte: Option<usize> = None;

        for child in node.children(&mut cursor) {
            if child.is_extra() {
                children.push(self.comment_leaf(child).with_spacing(Spacing::one_space()));
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            if child.kind() == "throws" {
                children.push(Block::leaf("throws"));
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            if child.kind() == "," {
                children.push(Block::leaf(",").with_spacing(Spacing::none()));
                prev_end_byte = Some(child.end_byte());
                continue;
            }

            let block = self.node(child);
            let w = Wrap {
                id: wrap_id,
                wrap_type,
                wrap_first_element: false,
                parent: outer_wrap_id,
                ignore_parent_wraps: false,
                cand_ignore_parents: false,
            };
            self.wraps.insert(w.id, w.clone());

            let source_break = if let Some(pb) = prev_end_byte {
                let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                gap.contains(&b'\n')
            } else {
                false
            };

            let type_indent = if has_outer_cont {
                IndentType::None
            } else {
                IndentType::Continuation
            };
            let block = if type_index == 0 {
                if source_break {
                    block
                        .with_wrap(w)
                        .with_indent(type_indent)
                        .with_spacing(Spacing::new_line())
                } else {
                    block.with_wrap(w).with_spacing(Spacing::one_space())
                }
            } else if source_break {
                block
                    .with_wrap(w)
                    .with_indent(type_indent)
                    .with_spacing(Spacing::new_line())
            } else {
                block
                    .with_wrap(w)
                    .with_indent(type_indent)
                    .with_spacing(Spacing::one_space())
            };

            children.push(block);
            type_index += 1;
            prev_end_byte = Some(child.end_byte());
        }

        Block::node(children)
    }
}
