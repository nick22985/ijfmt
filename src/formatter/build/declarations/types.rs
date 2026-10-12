use tree_sitter::Node;

use crate::config::WrapMode;
use crate::formatter::MalformedJavaMode;
use crate::formatter::build::util::wrap_type_from_mode;
use crate::formatter::build::{BlockBuilder, MalformedSupertypeRecovery};
use crate::formatter::ir::*;

fn malformed_supertype_recovery(node: Node) -> Option<MalformedSupertypeRecovery> {
    let mut class_cursor = node.walk();
    for clause in node.children(&mut class_cursor) {
        if !matches!(clause.kind(), "super_interfaces" | "extends_interfaces") {
            continue;
        }
        let mut clause_cursor = clause.walk();
        for list in clause.children(&mut clause_cursor) {
            if list.kind() != "type_list" {
                continue;
            }
            let mut list_cursor = list.walk();
            let children: Vec<Node> = list.children(&mut list_cursor).collect();
            for pair in children.windows(2) {
                let error = pair[0];
                let recovered = pair[1];
                if error.kind() != "ERROR" || recovered.is_extra() {
                    continue;
                }
                return match recovered.kind() {
                    "scoped_type_identifier" => Some(MalformedSupertypeRecovery::Scoped),
                    "generic_type" => Some(MalformedSupertypeRecovery::Generic),
                    "type_identifier" => Some(MalformedSupertypeRecovery::Simple),
                    _ => None,
                };
            }
        }
    }
    None
}

impl<'a> BlockBuilder<'a> {
    pub(crate) fn class_decl(&mut self, node: Node) -> Block {
        let local_recovery = if self.malformed_java == MalformedJavaMode::Preserve {
            malformed_supertype_recovery(node)
        } else {
            None
        };
        let saved_recovery =
            std::mem::replace(&mut self.malformed_supertype_recovery, local_recovery);

        let mut children = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_child: Option<Node> = None;

        let extends_wrap_type = wrap_type_from_mode(self.cfg.extends_list_wrap);

        for child in node.children(&mut cursor) {
            if child.is_extra() {
                let block = self.comment_leaf(child);
                let block = if child.kind() == "ERROR" {
                    block.with_indent(IndentType::Continuation)
                } else {
                    block
                };
                let block = if let Some(pb) = prev_end_byte {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.contains(&b'\n') {
                        block.with_spacing(Spacing::new_line())
                    } else {
                        block.with_spacing(Spacing::one_space())
                    }
                } else {
                    block
                };
                children.push(block);
                prev_end_byte = Some(child.end_byte());
                prev_child = Some(child);
                continue;
            }

            let block = match child.kind() {
                "superclass" | "super_interfaces" | "extends_interfaces" | "permits" => {
                    self.extends_list(child, extends_wrap_type)
                }
                "type_parameters" => self.node(child),
                "formal_parameters" => self.formal_parameters(child),
                "class_body"
                | "interface_body"
                | "enum_body"
                | "annotation_type_body"
                | "record_declaration_body" => self.class_body(child),
                _ => self.node(child),
            };

            let is_body = matches!(
                child.kind(),
                "class_body"
                    | "interface_body"
                    | "enum_body"
                    | "annotation_type_body"
                    | "record_declaration_body"
            );

            let spacing = if let Some(pb) = prev_end_byte {
                if child.kind() == "type_parameters"
                    || (node.kind() == "record_declaration" && child.kind() == "formal_parameters")
                {
                    Spacing::none()
                } else if is_body {
                    if self.malformed_supertype_recovery
                        == Some(MalformedSupertypeRecovery::Generic)
                    {
                        Spacing::blank_line()
                    } else {
                        use crate::config::BraceStyle;
                        match self.cfg.class_brace_style {
                            BraceStyle::NextLine => self.lbrace_new_line(child),
                            BraceStyle::EndOfLine => Spacing::one_space(),
                        }
                    }
                } else if (extends_wrap_type == WrapType::None
                    && matches!(
                        child.kind(),
                        "superclass" | "super_interfaces" | "extends_interfaces" | "permits"
                    )
                    && child.start_position().row != child.end_position().row
                    && self.src[pb..child.start_byte().min(self.src.len())].contains(&b'\n'))
                    || (matches!(
                        child.kind(),
                        "class" | "interface" | "enum" | "record" | "@interface"
                    ) && self.cfg.class_annotation_wrap != WrapMode::DoNotWrap
                        && prev_child.is_some_and(|p: Node| {
                            let mut w = p.walk();
                            p.kind() == "modifiers"
                                && p.children(&mut w).filter(|c| !c.is_extra()).count() > 0
                                && {
                                    let mut w2 = p.walk();
                                    p.children(&mut w2).filter(|c| !c.is_extra()).all(|c| {
                                        matches!(c.kind(), "marker_annotation" | "annotation")
                                    })
                                }
                        }))
                {
                    Spacing::new_line()
                } else if child.kind() == "@interface"
                    && prev_child.is_some_and(|p: Node| p.kind() == "modifiers")
                {
                    Spacing::one_space()
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

            let block = if prev_end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };

            prev_end_byte = Some(child.end_byte());
            prev_child = Some(child);
            children.push(block);
        }

        let result = Block::node(children);
        self.malformed_supertype_recovery = saved_recovery;
        result
    }

    fn extends_list(&mut self, node: Node, wrap_type: WrapType) -> Block {
        let mut children = Vec::new();
        let mut cursor = node.walk();
        let wrap_id = self.ids.wrap_id();
        let params_parent = self.last_params_wrap.take();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_end_row: Option<usize> = None;
        let keyword_on_own_line = {
            let mut i = node.start_byte();
            let mut own = false;
            while i > 0 {
                i -= 1;
                match self.src[i] {
                    b' ' | b'\t' => continue,
                    b'\n' => {
                        own = true;
                        break;
                    }
                    _ => break,
                }
            }
            own
        };

        for child in node.children(&mut cursor) {
            if child.is_extra() {
                if child.kind() == "ERROR" && self.malformed_java == MalformedJavaMode::Repair {
                    continue;
                }
                let spacing = if let Some(pb) = prev_end_byte {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.contains(&b'\n') {
                        Spacing::new_line()
                    } else {
                        Spacing::one_space()
                    }
                } else {
                    Spacing::one_space()
                };
                children.push(self.comment_leaf(child).with_spacing(spacing));
                prev_end_byte = Some(child.end_byte());
                prev_end_row = Some(child.end_position().row);
                continue;
            }

            let is_keyword = matches!(child.kind(), "extends" | "implements" | "permits" | "&");
            let is_comma = child.kind() == ",";

            if is_comma {
                children.push(Block::leaf(",").with_spacing(Spacing::none()));
                prev_end_byte = Some(child.end_byte());
                prev_end_row = Some(child.end_position().row);
                continue;
            }

            if child.kind() == "type_list" {
                let mut tl_cursor = child.walk();
                let mut follows_duplicate_error = false;
                let mut recovered_generic_suffix = false;
                for tl_child in child.children(&mut tl_cursor) {
                    if tl_child.kind() == "," {
                        children.push(Block::leaf(",").with_spacing(Spacing::none()));
                        prev_end_byte = Some(tl_child.end_byte());
                        prev_end_row = Some(tl_child.end_position().row);
                        continue;
                    }
                    if tl_child.is_extra() {
                        if tl_child.kind() == "ERROR"
                            && self.malformed_java == MalformedJavaMode::Repair
                        {
                            continue;
                        }
                        let spacing = if let Some(pb) = prev_end_byte {
                            let gap = &self.src[pb..tl_child.start_byte().min(self.src.len())];
                            if gap.contains(&b'\n') {
                                Spacing::new_line()
                            } else {
                                Spacing::one_space()
                            }
                        } else {
                            Spacing::one_space()
                        };
                        follows_duplicate_error = tl_child.kind() == "ERROR";
                        recovered_generic_suffix |= follows_duplicate_error
                            && matches!(
                                self.malformed_supertype_recovery,
                                Some(
                                    MalformedSupertypeRecovery::Generic
                                        | MalformedSupertypeRecovery::Simple
                                )
                            );
                        let error_block = if follows_duplicate_error
                            && self.malformed_supertype_recovery
                                == Some(MalformedSupertypeRecovery::Scoped)
                            && self.text(tl_child).lines().count() >= 3
                        {
                            let lines: Vec<_> = self.text(tl_child).lines().collect();
                            let last = lines.len() - 1;
                            Block::node(
                                lines
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, line)| {
                                        Block::leaf(line.trim().to_string())
                                            .with_indent(if index == last {
                                                IndentType::ColumnZero
                                            } else {
                                                IndentType::Continuation
                                            })
                                            .with_spacing(if index == 0 {
                                                spacing.clone()
                                            } else {
                                                Spacing::new_line()
                                            })
                                    })
                                    .collect(),
                            )
                        } else {
                            self.comment_leaf(tl_child)
                                .with_indent(IndentType::Continuation)
                                .with_spacing(spacing)
                        };
                        children.push(error_block);
                        prev_end_byte = Some(tl_child.end_byte());
                        prev_end_row = Some(tl_child.end_position().row);
                        continue;
                    }
                    let duplicate_recovered = std::mem::take(&mut follows_duplicate_error);
                    let block = if duplicate_recovered
                        && self.malformed_supertype_recovery
                            == Some(MalformedSupertypeRecovery::Scoped)
                    {
                        let text = self.text(tl_child);
                        if let Some((scope, name)) = text.rsplit_once('.') {
                            Block::node(vec![
                                Block::leaf(format!("{scope}."))
                                    .with_source_offset(tl_child.start_byte()),
                                Block::leaf(name.to_string())
                                    .with_indent(IndentType::ColumnZero)
                                    .with_spacing(Spacing::blank_line()),
                            ])
                        } else {
                            self.node(tl_child)
                        }
                    } else {
                        self.node(tl_child)
                    };
                    let w = Wrap {
                        id: wrap_id,
                        wrap_type,
                        wrap_first_element: false,
                        parent: None,
                        ignore_parent_wraps: false,
                        cand_ignore_parents: false,
                    };
                    self.wraps.insert(w.id, w.clone());
                    let source_break =
                        prev_end_row.is_some_and(|pr| tl_child.start_position().row > pr);
                    let suppress = source_break
                        && keyword_on_own_line
                        && children.last().is_some_and(|b: &Block| {
                            matches!(b.text.as_str(), "extends" | "implements" | "permits" | "&")
                        });
                    let spacing = if source_break && !suppress {
                        Spacing::new_line()
                    } else if recovered_generic_suffix
                        && children.last().is_some_and(|block| block.text == ",")
                    {
                        Spacing::none()
                    } else {
                        Spacing::one_space()
                    };
                    children.push(
                        block
                            .with_wrap(w)
                            .with_indent(IndentType::Continuation)
                            .with_spacing(spacing),
                    );
                    prev_end_byte = Some(tl_child.end_byte());
                    prev_end_row = Some(tl_child.end_position().row);
                }
                continue;
            }

            let block = self.node(child);

            let source_break = prev_end_row.is_some_and(|pr| child.start_position().row > pr);

            let w = Wrap {
                id: wrap_id,
                wrap_type,
                wrap_first_element: false,
                parent: params_parent,
                ignore_parent_wraps: false,
                cand_ignore_parents: false,
            };
            self.wraps.insert(w.id, w.clone());

            let block = if is_keyword {
                let block = if prev_end_byte.is_some() {
                    block.with_spacing(Spacing::one_space())
                } else {
                    block
                };
                block.with_wrap(w).with_indent(IndentType::Continuation)
            } else {
                let suppress_type_break = source_break
                    && keyword_on_own_line
                    && children.last().is_some_and(|b: &Block| {
                        matches!(b.text.as_str(), "extends" | "implements" | "permits" | "&")
                    });
                let spacing = if source_break && !suppress_type_break {
                    Spacing::new_line()
                } else {
                    Spacing::one_space()
                };
                block
                    .with_wrap(w)
                    .with_indent(IndentType::Continuation)
                    .with_spacing(spacing)
            };

            children.push(block);
            prev_end_byte = Some(child.end_byte());
            prev_end_row = Some(child.end_position().row);
        }

        Block::node(children)
    }

    pub(in crate::formatter::build) fn enum_constant(&mut self, node: Node) -> Block {
        let mut parts = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut had_trailing_line_comment_after_modifiers = false;
        let mut prev_was_modifiers = false;
        for child in node.children(&mut cursor) {
            if child.is_extra() {
                if prev_was_modifiers && child.kind() == "line_comment" {
                    had_trailing_line_comment_after_modifiers = true;
                }
                let spacing = if let Some(pb) = prev_end_byte {
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.contains(&b'\n') {
                        Spacing::new_line()
                    } else {
                        Spacing::one_space()
                    }
                } else {
                    Spacing::one_space()
                };
                parts.push(self.comment_leaf(child).with_spacing(spacing));
                prev_end_byte = Some(child.end_byte());
                prev_was_modifiers = false;
                continue;
            }
            let block = match child.kind() {
                "argument_list" => self.argument_list(child),
                "class_body" => {
                    let indent =
                        if self.cfg.class_brace_style == crate::config::BraceStyle::NextLine {
                            IndentType::Continuation
                        } else {
                            IndentType::None
                        };
                    self.class_body(child).with_indent(indent)
                }
                _ => self.node(child),
            };
            let block = if had_trailing_line_comment_after_modifiers
                && child.kind() != "modifiers"
                && child.kind() != "argument_list"
                && child.kind() != "class_body"
            {
                had_trailing_line_comment_after_modifiers = false;
                block.with_indent(IndentType::Continuation)
            } else {
                block
            };
            let spacing = if let Some(pb) = prev_end_byte {
                if child.kind() == "argument_list" {
                    Spacing::none()
                } else if child.kind() == "class_body" {
                    if self.cfg.class_brace_style == crate::config::BraceStyle::NextLine {
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
            let block = if prev_end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };
            prev_was_modifiers = child.kind() == "modifiers";
            prev_end_byte = Some(child.end_byte());
            parts.push(block);
        }
        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Block::node(parts)
        }
    }

    pub(in crate::formatter::build) fn type_parameters(&mut self, node: Node) -> Block {
        let mut parts = Vec::new();
        let mut cursor = node.walk();
        let mut prev_end_byte: Option<usize> = None;
        let mut prev_end_row: Option<usize> = None;
        for child in node.children(&mut cursor) {
            if child.is_extra() {
                parts.push(self.comment_leaf(child).with_spacing(Spacing::one_space()));
                prev_end_byte = Some(child.end_byte());
                prev_end_row = Some(child.end_position().row);
                continue;
            }
            let mut block = self.node(child);
            let spacing = if prev_end_byte.is_none() || child.kind() == "," {
                Spacing::none()
            } else {
                let on_new_line = prev_end_row.is_some_and(|pr| child.start_position().row > pr);
                if on_new_line {
                    Spacing::new_line()
                } else if prev_end_byte.is_some()
                    && parts.last().is_some_and(|b: &Block| b.text == ",")
                {
                    Spacing::one_space()
                } else {
                    let pb = prev_end_byte.unwrap_or(0);
                    let gap = &self.src[pb..child.start_byte().min(self.src.len())];
                    if gap.iter().any(|&b| b == b' ' || b == b'\t') {
                        Spacing::one_space()
                    } else {
                        Spacing::none()
                    }
                }
            };
            let on_new_line = prev_end_row.is_some_and(|pr| child.start_position().row > pr);
            if on_new_line && !matches!(child.kind(), "<" | ",") {
                block = block.with_indent(IndentType::Continuation);
            }
            let block = if prev_end_byte.is_some() {
                block.with_spacing(spacing)
            } else {
                block
            };
            prev_end_byte = Some(child.end_byte());
            prev_end_row = Some(child.end_position().row);
            parts.push(block);
        }
        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Block::node(parts)
        }
    }
}
