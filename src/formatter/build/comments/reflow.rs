use super::text::{split_tag_prefix, starts_with_todo, tag_name_end, wrap_text_tokens};
use std::collections::HashMap;

fn compute_aligned_name_widths(lines: &[String]) -> HashMap<usize, usize> {
    let mut result: HashMap<usize, usize> = HashMap::new();
    let named_tags = ["@param ", "@throws ", "@exception "];

    let mut current_group: Vec<(usize, usize)> = Vec::new();
    let mut current_keyword: &str = "";
    let mut in_tag_run = false;

    let flush_group = |group: &mut Vec<(usize, usize)>, result: &mut HashMap<usize, usize>| {
        if group.len() >= 2 {
            let max_len = group.iter().map(|(_, l)| *l).max().unwrap_or(0);
            for (idx, _) in group.iter() {
                result.insert(*idx, max_len);
            }
        }
        group.clear();
    };

    let mut i = 1;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == "*/" || trimmed == "**/" {
            break;
        }
        let content = trimmed
            .strip_prefix("* ")
            .or_else(|| {
                if trimmed == "*" {
                    Some("")
                } else {
                    trimmed.strip_prefix("*")
                }
            })
            .unwrap_or(trimmed);

        let matched_tag = named_tags
            .iter()
            .find(|&&t| content.starts_with(t))
            .copied();
        if let Some(tag) = matched_tag {
            if in_tag_run && tag != current_keyword {
                flush_group(&mut current_group, &mut result);
            }
            let name_len = {
                let after_tag = &content[tag.len()..];
                tag_name_end(after_tag)
            };
            current_group.push((i, name_len));
            current_keyword = tag;
            in_tag_run = true;
        } else if in_tag_run {
        }
        i += 1;
    }
    flush_group(&mut current_group, &mut result);

    result
}

pub(super) fn reflow_javadoc_text(
    lines: &mut Vec<String>,
    max_width: usize,
    indent: &str,
    align_params: bool,
) {
    let aligned_name_widths = if align_params {
        compute_aligned_name_widths(lines)
    } else {
        std::collections::HashMap::new()
    };

    let mut result: Vec<String> = Vec::new();
    let mut i = 0;
    let mut in_pre = false;
    let mut keep_indents = false;
    let mut in_todo = false;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if i == 0 || trimmed == "*/" || trimmed == "**/" {
            result.push(lines[i].clone());
            i += 1;
            continue;
        }

        let content = trimmed
            .strip_prefix("* ")
            .or_else(|| {
                if trimmed == "*" {
                    Some("")
                } else {
                    trimmed.strip_prefix("*")
                }
            })
            .unwrap_or(trimmed);

        let content_lower = content.to_lowercase();

        let opens_pre = content_lower.contains("<pre>") || content_lower.contains("<pre ");
        let closes_pre = content_lower.contains("</pre>");
        if in_pre {
            if closes_pre {
                in_pre = false;
            }
            result.push(lines[i].clone());
            i += 1;
            continue;
        }
        if opens_pre && !closes_pre {
            in_pre = true;
            result.push(lines[i].clone());
            i += 1;
            continue;
        }

        if content.contains("{@snippet") && content.contains(':') {
            result.push(lines[i].clone());
            i += 1;
            while i < lines.len() {
                let t = lines[i].trim();
                result.push(lines[i].clone());
                let c = t
                    .strip_prefix("* ")
                    .or_else(|| t.strip_prefix("*"))
                    .unwrap_or(t);
                i += 1;
                if c.trim() == "}" || c.trim_end().ends_with('}') {
                    break;
                }
            }
            continue;
        }

        if content.starts_with('@') {
            in_todo = false;
        } else if !in_todo && starts_with_todo(content) {
            in_todo = true;
        }
        if in_todo {
            result.push(lines[i].clone());
            i += 1;
            continue;
        }

        if content.starts_with('@') {
            keep_indents = false;
        } else if !keep_indents
            && let Some(tag) = html_start_tag(content)
            && matches!(tag, "table" | "ol" | "ul" | "div" | "dl")
        {
            keep_indents = true;
        }

        if keep_indents {
            result.push(lines[i].clone());
            i += 1;
            continue;
        }

        if trimmed == "*" || content == "<p>" || content == "<p/>" {
            result.push(lines[i].clone());
            i += 1;
            continue;
        }

        if content.starts_with('@') {
            let (tag_prefix, description) = split_tag_prefix(content);
            let mut full_desc = description.to_string();

            let tag_name_only = content
                .strip_prefix('@')
                .unwrap_or(content)
                .split([' ', '\t'])
                .next()
                .unwrap_or("");
            let is_known_tag = matches!(
                tag_name_only,
                "see"
                    | "since"
                    | "version"
                    | "deprecated"
                    | "return"
                    | "param"
                    | "throws"
                    | "exception"
                    | "author"
            );

            let mut j = i + 1;
            while j < lines.len() && !(is_known_tag && starts_with_html_tag(&full_desc)) {
                let nt = lines[j].trim();
                if nt == "*/" || nt == "**/" {
                    break;
                }
                let nc = nt
                    .strip_prefix("* ")
                    .or_else(|| {
                        if nt == "*" {
                            Some("")
                        } else {
                            nt.strip_prefix("*")
                        }
                    })
                    .unwrap_or(nt);

                if nc.is_empty() || nc.starts_with('@') || nc == "<p>" || nc == "<p/>" {
                    break;
                }
                if starts_with_html_tag(nc) || starts_with_todo(nc) || line_has_unclosed_pre_tag(nc)
                {
                    break;
                }
                full_desc.push(' ');
                full_desc.push_str(nc.trim());
                j += 1;
            }

            let mut extra_paras: Vec<(bool, String)> = Vec::new();
            if is_known_tag {
                loop {
                    if j >= lines.len() {
                        break;
                    }
                    let nt = lines[j].trim();
                    if nt == "*/" || nt == "**/" {
                        break;
                    }
                    let nc = nt
                        .strip_prefix("* ")
                        .or_else(|| {
                            if nt == "*" {
                                Some("")
                            } else {
                                nt.strip_prefix("*")
                            }
                        })
                        .unwrap_or(nt);
                    if nc == "<p>" || nc == "<p/>" {
                        extra_paras.push((true, nc.to_string()));
                        j += 1;
                        continue;
                    }
                    if nc.is_empty()
                        || nc.starts_with('@')
                        || starts_with_todo(nc)
                        || line_has_unclosed_pre_tag(nc)
                    {
                        break;
                    }
                    if starts_with_html_tag(nc) {
                        extra_paras.push((false, nc.trim().to_string()));
                        j += 1;
                        continue;
                    }
                    let mut para = String::new();
                    while j < lines.len() {
                        let nt2 = lines[j].trim();
                        if nt2 == "*/" || nt2 == "**/" {
                            break;
                        }
                        let nc2 = nt2
                            .strip_prefix("* ")
                            .or_else(|| {
                                if nt2 == "*" {
                                    Some("")
                                } else {
                                    nt2.strip_prefix("*")
                                }
                            })
                            .unwrap_or(nt2);
                        if nc2.is_empty()
                            || nc2.starts_with('@')
                            || nc2 == "<p>"
                            || nc2 == "<p/>"
                            || starts_with_html_tag(nc2)
                            || starts_with_todo(nc2)
                            || line_has_unclosed_pre_tag(nc2)
                        {
                            break;
                        }
                        if !para.is_empty() {
                            para.push(' ');
                        }
                        para.push_str(nc2.trim());
                        j += 1;
                    }
                    if para.is_empty() {
                        break;
                    }
                    extra_paras.push((false, para));
                }
            }

            let is_named_tag = align_params
                && (content.starts_with("@param ")
                    || content.starts_with("@throws ")
                    || content.starts_with("@exception ")
                    || content.starts_with("@serialField "));
            let tag_prefix_len = tag_prefix.len();

            let effective_prefix_len = if is_named_tag {
                if let Some(&max_name_len) = aligned_name_widths.get(&i) {
                    let kw_len = ["@param ", "@throws ", "@exception ", "@serialField "]
                        .iter()
                        .find(|&&kw| content.starts_with(kw))
                        .map(|kw| kw.len())
                        .unwrap_or(0);
                    kw_len + max_name_len + 1
                } else {
                    tag_prefix_len
                }
            } else {
                tag_prefix_len
            };

            let first_line_width = max_width.saturating_sub(effective_prefix_len);
            let (cont_width, cont_prefix) = if is_named_tag {
                (first_line_width, " ".repeat(effective_prefix_len))
            } else {
                (max_width, String::new())
            };

            if !is_known_tag {
                let whole = format!("{}{}", tag_prefix, full_desc);
                for wline in wrap_text_tokens(&whole, max_width, max_width) {
                    result.push(format!("{}* {}", indent, wline));
                }
            } else if full_desc.is_empty() {
                result.push(format!("{}* {}", indent, tag_prefix.trim_end()));
            } else {
                if std::env::var("IJFMT_JD_DBG").is_ok() {
                    eprintln!(
                        "[JD] tag={:?} named={} eff={} maxw={} first={} cont={} desc={:?}",
                        tag_prefix,
                        is_named_tag,
                        effective_prefix_len,
                        max_width,
                        first_line_width,
                        cont_width,
                        full_desc
                    );
                }
                let wrapped = wrap_text_tokens(&full_desc, first_line_width, cont_width);
                for (k, wline) in wrapped.iter().enumerate() {
                    if k == 0 {
                        result.push(format!("{}* {}{}", indent, tag_prefix, wline));
                    } else {
                        result.push(format!("{}* {}{}", indent, cont_prefix, wline));
                    }
                }
            }

            for (marked, para) in &extra_paras {
                if *marked && para.chars().count() < cont_width {
                    result.push(format!("{}* {}{}", indent, cont_prefix, para));
                } else {
                    for wline in wrap_text_tokens(para, cont_width, cont_width) {
                        result.push(format!("{}* {}{}", indent, cont_prefix, wline));
                    }
                }
            }

            i = j;
            continue;
        }

        if content.starts_with('<') && !content.starts_with("<p>") {
            let line_overflows = content.chars().count() >= max_width;
            if line_overflows {
                let wrapped = wrap_text_tokens(content, max_width, max_width);
                for wline in &wrapped {
                    result.push(format!("{}* {}", indent, wline));
                }
                i += 1;
                continue;
            }
            result.push(lines[i].clone());
            i += 1;
            continue;
        }

        if content.starts_with("<p>") && content.len() > 3 {
            if content.chars().count() < max_width {
                result.push(lines[i].clone());
            } else {
                for wline in wrap_text_tokens(content, max_width, max_width) {
                    result.push(format!("{}* {}", indent, wline));
                }
            }
            {
                let mut j = i + 1;
                let mut cont_text = String::new();
                while j < lines.len() {
                    let nt = lines[j].trim();
                    if nt == "*/" || nt == "**/" {
                        break;
                    }
                    let nc = nt
                        .strip_prefix("* ")
                        .or_else(|| {
                            if nt == "*" {
                                Some("")
                            } else {
                                nt.strip_prefix("*")
                            }
                        })
                        .unwrap_or(nt);
                    if nc.is_empty()
                        || nc.starts_with('@')
                        || nc == "<p>"
                        || nc == "<p/>"
                        || starts_with_html_tag(nc)
                        || starts_with_todo(nc)
                        || line_has_unclosed_pre_tag(nc)
                    {
                        break;
                    }
                    if !cont_text.is_empty() {
                        cont_text.push(' ');
                    }
                    cont_text.push_str(nc.trim());
                    j += 1;
                }
                if !cont_text.is_empty() {
                    let wrapped = wrap_text_tokens(&cont_text, max_width, max_width);
                    for wline in &wrapped {
                        result.push(format!("{}* {}", indent, wline));
                    }
                }
                i = j;
                continue;
            }
        }

        let mut para_text = content.to_string();
        let mut j = i + 1;
        while j < lines.len() {
            let nt = lines[j].trim();
            if nt == "*/" || nt == "**/" {
                break;
            }
            let nc = nt
                .strip_prefix("* ")
                .or_else(|| {
                    if nt == "*" {
                        Some("")
                    } else {
                        nt.strip_prefix("*")
                    }
                })
                .unwrap_or(nt);

            if nc.is_empty()
                || nc.starts_with('@')
                || nc == "<p>"
                || nc == "<p/>"
                || starts_with_html_tag(nc)
                || starts_with_todo(nc)
                || line_has_unclosed_pre_tag(nc)
            {
                break;
            }
            para_text.push(' ');
            para_text.push_str(nc.trim());
            j += 1;
        }

        let wrapped = wrap_text_tokens(&para_text, max_width, max_width);
        for wline in &wrapped {
            result.push(format!("{}* {}", indent, wline));
        }

        i = j;
    }

    *lines = result;
}

fn line_has_unclosed_pre_tag(line: &str) -> bool {
    let l = line.to_lowercase();
    let opens = l.matches("<pre>").count() + l.matches("<pre ").count();
    let closes = l.matches("</pre>").count();
    opens > closes
}

fn html_start_tag(line: &str) -> Option<&str> {
    if !starts_with_html_tag(line) {
        return None;
    }
    let start = line.find('<')? + 1;
    let bytes = line.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        let ch = bytes[i];
        if !(ch.is_ascii_alphabetic() || (i > start && (ch.is_ascii_digit() || ch == b'-'))) {
            return Some(&line[start..i]);
        }
        i += 1;
    }
    None
}

fn starts_with_html_tag(line: &str) -> bool {
    let s = line.trim_start().as_bytes();
    if s.first() != Some(&b'<') {
        return false;
    }
    let mut i = 1;
    if s.get(i) == Some(&b'/') {
        i += 1;
    }
    let name_start = i;
    while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == b'_') {
        i += 1;
    }
    if i == name_start {
        return false;
    }
    while i < s.len() && s[i] == b' ' {
        i += 1;
    }
    match s.get(i) {
        Some(&b'>') => true,
        Some(&b) if b.is_ascii_alphanumeric() || b == b'_' => {
            let mut k = i;
            while k < s.len() && (s[k].is_ascii_alphanumeric() || s[k] == b'_') {
                k += 1;
            }
            while k < s.len() && s[k] == b' ' {
                k += 1;
            }
            s.get(k) == Some(&b'=') && s[k..].contains(&b'>')
        }
        _ => false,
    }
}
