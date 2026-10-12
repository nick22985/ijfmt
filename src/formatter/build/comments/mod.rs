pub(super) mod nodes;
mod normalize;
mod reflow;
mod tags;
#[cfg(test)]
mod tests;
mod text;

use self::normalize::{
    expand_single_line_javadoc, insert_p_tags, normalize_extra_spaces_after_star,
    normalize_see_commas, strip_space_after_broken_doc_ref,
};
use self::reflow::reflow_javadoc_text;
use self::tags::{
    align_tag_descriptions, collapse_blank_lines_in_tag_section,
    ensure_blank_line_before_first_tag, join_tag_continuation_lines, normalize_exception_to_throws,
    reorder_javadoc_tags,
};
pub(super) use normalize::normalize_doc_ref_commas;

use super::BlockBuilder;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum JdOwner {
    Method,
    Class,
    Other,
}

impl JdOwner {
    fn handles_param(self) -> bool {
        matches!(self, JdOwner::Method | JdOwner::Class)
    }
}

impl<'a> BlockBuilder<'a> {
    pub(crate) fn process_javadoc(&self, raw_text: &str, output_col: usize) -> String {
        self.process_javadoc_for(raw_text, output_col, JdOwner::Method)
    }

    pub(crate) fn process_javadoc_for(
        &self,
        raw_text: &str,
        output_col: usize,
        owner: JdOwner,
    ) -> String {
        let is_javadoc = raw_text.starts_with("/**") && !raw_text.starts_with("/***");
        if !is_javadoc {
            return raw_text.to_string();
        }

        let is_single_line = !raw_text.contains('\n');

        let text = if is_single_line {
            expand_single_line_javadoc(raw_text)
        } else {
            raw_text.to_string()
        };

        let text = normalize_doc_ref_commas(&text);

        let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();

        if !is_single_line && lines.len() > 1 {
            let first = lines[0].clone();
            let trailing = first
                .trim_start()
                .strip_prefix("/**")
                .map(|r| r.trim())
                .unwrap_or("");
            if !trailing.is_empty() {
                let base: String = first
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                lines[0] = format!("{}/**", base);
                lines.insert(1, format!("{} * {}", base, trailing));
            }
        }

        if !is_single_line && lines.len() > 1 {
            let last_idx = lines.len() - 1;
            let last = lines[last_idx].trim_end().to_string();
            if let Some(body) = last.strip_suffix('/') {
                let stars = body.len() - body.trim_end_matches('*').len();
                let before = &body[..body.len() - stars];
                if stars > 0 && !before.trim().is_empty() {
                    let base: String = lines[0]
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect();
                    lines[last_idx] = before.trim_end().to_string();
                    lines.push(format!("{} {}/", base, "*".repeat(stars)));
                }
            }
        }

        if lines.len() == 2 {
            let first_trimmed = lines[0].trim();
            let second_trimmed = lines[1].trim();
            if first_trimmed == "/**" && (second_trimmed == "*/" || second_trimmed == "**/") {
                let indent: String = lines[1]
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                lines.insert(1, format!("{}*", indent));
            }
        }

        for line in lines.iter_mut().skip(1) {
            let trimmed = line.trim_start();
            if trimmed.starts_with('*')
                && !trimmed.starts_with("* ")
                && !trimmed.starts_with("*/")
                && !trimmed.starts_with("**/")
                && trimmed != "*"
            {
                let indent: String = line
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                let rest = &trimmed[1..];
                *line = format!("{}* {}", indent, rest);
            }
        }

        let has_star = lines.iter().skip(1).any(|l| {
            let t = l.trim();
            (t.starts_with("* ") || t == "*") && t != "*/" && t != "**/"
        });
        {
            if has_star {
                let star_indent: String = lines
                    .iter()
                    .skip(1)
                    .find(|l| {
                        let t = l.trim();
                        t.starts_with("* ") || t == "*"
                    })
                    .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\t').collect())
                    .unwrap_or_default();
                for line in lines.iter_mut().skip(1) {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('*') {
                        continue;
                    }
                    *line = format!("{}* {}", star_indent, trimmed);
                }
            }
        }

        {
            let is_blank = |l: &String| l.trim().is_empty();
            let closer_ok = lines.last().is_some_and(|l| {
                let t = l.trim();
                t.starts_with("*/") || t.starts_with("**/")
            });
            let has_real_content =
                lines.len() > 2 && lines[1..lines.len() - 1].iter().any(|l| !is_blank(l));
            if closer_ok && has_real_content {
                while lines.len() > 2 && is_blank(&lines[lines.len() - 2]) {
                    lines.remove(lines.len() - 2);
                }
                while lines.len() > 2 && is_blank(&lines[1]) {
                    lines.remove(1);
                }
            }
        }

        if self.cfg.jd_p_at_empty_lines {
            insert_p_tags(&mut lines);
        }

        normalize_exception_to_throws(&mut lines);

        ensure_blank_line_before_first_tag(&mut lines);

        collapse_blank_lines_in_tag_section(&mut lines);

        reorder_javadoc_tags(&mut lines, owner);

        normalize_extra_spaces_after_star(&mut lines);

        normalize_see_commas(&mut lines);

        join_tag_continuation_lines(&mut lines);

        strip_space_after_broken_doc_ref(&mut lines);

        if self.cfg.wrap_comments && self.cfg.right_margin > 0 && has_star {
            let star_indent: String = lines
                .iter()
                .skip(1)
                .find(|l| l.trim().starts_with("* ") || l.trim() == "*")
                .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\t').collect())
                .unwrap_or_else(|| {
                    let base: String = lines
                        .first()
                        .unwrap_or(&String::new())
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect();
                    format!("{} ", base)
                });
            let prefix_len = output_col + 3;
            let max_width = if self.cfg.right_margin > prefix_len {
                self.cfg.right_margin - prefix_len
            } else {
                40
            };
            reflow_javadoc_text(
                &mut lines,
                max_width,
                &star_indent,
                self.cfg.jd_align_param_comments && owner.handles_param(),
            );
        }

        if self.cfg.jd_align_param_comments && owner.handles_param() {
            align_tag_descriptions(&mut lines, "@param ");
        }
        if self.cfg.jd_align_exception_comments && owner == JdOwner::Method {
            align_tag_descriptions(&mut lines, "@throws ");
        }

        lines.join("\n")
    }
}
