use super::JdOwner;
use super::text::{split_tag_prefix, tag_name_end};

pub(super) fn normalize_exception_to_throws(lines: &mut [String]) {
    for line in lines.iter_mut() {
        if let Some(pos) = line.find("@exception ") {
            let before = &line[..pos];
            if before.trim_end().ends_with('*') {
                let new_line = format!(
                    "{}@throws {}",
                    &line[..pos],
                    &line[pos + "@exception ".len()..]
                );
                *line = new_line;
            }
        }
    }
}

pub(super) fn ensure_blank_line_before_first_tag(lines: &mut Vec<String>) {
    if lines.len() <= 2 {
        return;
    }

    let mut first_tag_idx: Option<usize> = None;
    let mut in_pre = false;

    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let trimmed = line.trim();
        if trimmed == "*/" || trimmed == "**/" {
            break;
        }
        let content = trimmed
            .strip_prefix("* ")
            .or_else(|| trimmed.strip_prefix("*"))
            .unwrap_or(trimmed)
            .trim_start();

        let content_lower = content.to_lowercase();
        if content_lower.contains("<pre>") || content_lower.contains("<pre ") {
            in_pre = true;
        }
        if content_lower.contains("</pre>") {
            in_pre = false;
            continue;
        }
        if in_pre {
            continue;
        }

        if content.starts_with('@') {
            first_tag_idx = Some(i);
            break;
        }
    }

    if let Some(tag_i) = first_tag_idx
        && tag_i > 0
    {
        let is_blank = |s: &str| {
            let t = s.trim();
            t.is_empty() || t == "*"
        };
        let prev = lines[tag_i - 1].trim();
        let has_blank_before = is_blank(prev);
        let is_first_content = tag_i == 1 || (tag_i == 2 && lines[1].trim().is_empty());
        if !has_blank_before && !is_first_content {
            let indent: String = lines[tag_i]
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            lines.insert(tag_i, format!("{}*", indent));
        }

        let mut new_tag_i = None;
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                continue;
            }
            let trimmed = line.trim();
            let content = trimmed
                .strip_prefix("* ")
                .or_else(|| trimmed.strip_prefix("*"))
                .unwrap_or(trimmed)
                .trim_start();
            if content.starts_with('@') {
                new_tag_i = Some(i);
                break;
            }
        }
        if let Some(ti) = new_tag_i
            && ti >= 2
            && is_blank(&lines[ti - 1])
        {
            let mut blank_count: usize = 1;
            let mut j = ti - 2;
            loop {
                if is_blank(&lines[j]) {
                    blank_count += 1;
                } else {
                    break;
                }
                if j == 0 {
                    break;
                }
                j -= 1;
            }
            let preceding = lines[j].trim();
            let max_blanks = if preceding == "/**" { 2usize } else { 1 };
            let to_remove = blank_count.saturating_sub(max_blanks);
            for _ in 0..to_remove {
                lines.remove(j + 1);
            }
            for k in j + 1..(j + 1 + blank_count - to_remove).min(lines.len()) {
                if lines[k].trim().is_empty() {
                    let indent: String = lines[ti.min(lines.len() - 1)]
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect();
                    lines[k] = format!("{}*", indent);
                }
            }
        }
    }
}

pub(super) fn collapse_blank_lines_in_tag_section(lines: &mut Vec<String>) {
    let is_inside_pre = |lines: &[String], idx: usize| -> bool {
        let mut depth = 0i32;
        for line in lines.iter().take(idx) {
            let lc = line.to_lowercase();
            if lc.contains("<pre>") || lc.contains("<pre ") {
                depth += 1;
            }
            if lc.contains("</pre>") {
                depth -= 1;
            }
        }
        depth > 0
    };

    let mut i = 1;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == "*/" || trimmed == "**/" {
            break;
        }

        if trimmed == "*" {
            if is_inside_pre(lines, i) {
                i += 1;
                continue;
            }

            let in_tag_section = {
                let mut found = false;
                let mut j = i;
                while j > 0 {
                    j -= 1;
                    if is_inside_pre(lines, j) {
                        continue;
                    }
                    let pt = lines[j].trim();
                    if pt == "*" || pt.is_empty() {
                        continue;
                    }
                    if pt == "/**" {
                        break;
                    }
                    let pc = pt
                        .strip_prefix("* ")
                        .or_else(|| pt.strip_prefix("*"))
                        .unwrap_or(pt)
                        .trim_start();
                    if pc.starts_with('@') {
                        found = true;
                        break;
                    }
                    continue;
                }
                found
            };

            let next_is_tag = {
                let mut found = false;
                let mut j = i + 1;
                while j < lines.len() {
                    if is_inside_pre(lines, j) {
                        j += 1;
                        continue;
                    }
                    let nt = lines[j].trim();
                    if nt == "*" || nt.is_empty() {
                        j += 1;
                        continue;
                    }
                    if nt == "*/" || nt == "**/" {
                        break;
                    }
                    let nc = nt
                        .strip_prefix("* ")
                        .or_else(|| nt.strip_prefix("*"))
                        .unwrap_or(nt)
                        .trim_start();
                    found = nc.starts_with('@');
                    break;
                }
                found
            };

            if in_tag_section && next_is_tag {
                lines.remove(i);
                continue;
            }
        }
        i += 1;
    }
}

pub(super) fn reorder_javadoc_tags(lines: &mut [String], owner: JdOwner) {
    let mut tag_start: Option<usize> = None;
    let mut tag_end: Option<usize> = None;
    let mut in_pre = false;

    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let trimmed = line.trim();
        if trimmed == "*/" || trimmed == "**/" {
            if tag_start.is_some() {
                tag_end = Some(i);
            }
            break;
        }
        let content = trimmed
            .strip_prefix("* ")
            .or_else(|| trimmed.strip_prefix("*"))
            .unwrap_or(trimmed)
            .trim_start();
        let content_lower = content.to_lowercase();
        if content_lower.contains("<pre>") || content_lower.contains("<pre ") {
            in_pre = true;
        }
        if content_lower.contains("</pre>") {
            in_pre = false;
        }
        if !in_pre && content.starts_with('@') && tag_start.is_none() {
            tag_start = Some(i);
        }
    }

    let (start, mut end) = match (tag_start, tag_end) {
        (Some(s), Some(e)) if s < e => (s, e),
        _ => return,
    };

    while end > start && lines[end - 1].trim() == "*" {
        end -= 1;
    }
    if end <= start {
        return;
    }

    let mut blocks: Vec<(usize, Vec<String>)> = Vec::new();
    let mut current_block: Vec<String> = Vec::new();
    let mut current_key: usize = 99;

    let tag_order = |content: &str| -> usize {
        if content.starts_with("@param") {
            if owner.handles_param() { 2 } else { 5 }
        } else if content.starts_with("@return") {
            if owner == JdOwner::Method { 3 } else { 5 }
        } else if content.starts_with("@throws") || content.starts_with("@exception") {
            if owner == JdOwner::Method { 4 } else { 5 }
        } else if content.starts_with("@author") {
            if owner == JdOwner::Class { 6 } else { 5 }
        } else if content.starts_with("@version") {
            if owner == JdOwner::Class { 7 } else { 5 }
        } else if content.starts_with("@see") {
            8
        } else if content.starts_with("@since") {
            9
        } else if content.starts_with("@serial ") || content == "@serial" {
            10
        } else if content.starts_with("@serialField") {
            11
        } else if content.starts_with("@serialData") {
            12
        } else if content.starts_with("@deprecated") {
            13
        } else {
            5
        }
    };

    let mut in_pre2 = false;
    for line in lines.iter().take(end).skip(start) {
        let trimmed = line.trim();
        let content = trimmed
            .strip_prefix("* ")
            .or_else(|| trimmed.strip_prefix("*"))
            .unwrap_or(trimmed)
            .trim_start();

        let content_lower = content.to_lowercase();
        if content_lower.contains("<pre>") || content_lower.contains("<pre ") {
            in_pre2 = true;
        }
        if content_lower.contains("</pre>") {
            in_pre2 = false;
        }

        if !in_pre2 && content.starts_with('@') {
            if !current_block.is_empty() {
                blocks.push((current_key, std::mem::take(&mut current_block)));
            }
            current_key = tag_order(content);
        }
        current_block.push(line.clone());
    }
    if !current_block.is_empty() {
        blocks.push((current_key, current_block));
    }

    let already_sorted = blocks.windows(2).all(|w| w[0].0 <= w[1].0);
    if already_sorted {
        return;
    }

    blocks.sort_by_key(|(key, _)| *key);

    let mut new_lines: Vec<String> = Vec::new();
    for (_, block_lines) in &blocks {
        new_lines.extend(block_lines.iter().cloned());
    }

    let old_len = end - start;
    if new_lines.len() == old_len {
        for (i, new_line) in new_lines.into_iter().enumerate() {
            lines[start + i] = new_line;
        }
    }
}

pub(super) fn join_tag_continuation_lines(lines: &mut Vec<String>) {
    let mut i = 1;
    while i + 1 < lines.len() {
        let trimmed = lines[i].trim();
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

        if content.starts_with('@') {
            let (_, desc) = split_tag_prefix(content);
            if desc.trim().is_empty() {
                let next_trimmed = lines[i + 1].trim();
                if next_trimmed == "*/" || next_trimmed == "**/" || next_trimmed == "*" {
                    i += 1;
                    continue;
                }
                let next_content = next_trimmed
                    .strip_prefix("* ")
                    .or_else(|| {
                        if next_trimmed == "*" {
                            Some("")
                        } else {
                            next_trimmed.strip_prefix("*")
                        }
                    })
                    .unwrap_or(next_trimmed);
                if !next_content.is_empty() && !next_content.starts_with('@') {
                    let indent: String = lines[i]
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect();
                    lines[i] = format!("{}* {} {}", indent, content, next_content);
                    lines.remove(i + 1);
                    i += 1;
                    continue;
                }
            }
        }
        i += 1;
    }
}

pub(super) fn align_tag_descriptions(lines: &mut Vec<String>, tag_keyword: &str) {
    {
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

            if let Some(after_tag) = content.strip_prefix(tag_keyword) {
                let name_end = tag_name_end(after_tag);
                let desc = if name_end < after_tag.len() {
                    after_tag[name_end..].trim_start()
                } else {
                    ""
                };

                if desc.is_empty() {
                    let mut j = i + 1;
                    while j < lines.len() {
                        let jt = lines[j].trim();
                        if jt == "*/" || jt == "**/" || jt == "*" {
                            break;
                        }
                        let jc = jt
                            .strip_prefix("* ")
                            .or_else(|| jt.strip_prefix("*"))
                            .unwrap_or(jt);
                        if jc.starts_with('@') {
                            break;
                        }
                        let text = jc.trim();
                        if !text.is_empty() {
                            let indent: String = lines[i]
                                .chars()
                                .take_while(|c| *c == ' ' || *c == '\t')
                                .collect();
                            let name = &after_tag[..name_end];
                            lines[i] = format!("{}* {}{} {}", indent, tag_keyword, name, text);
                            lines.remove(j);
                            break;
                        }
                        j += 1;
                    }
                }
            }
            i += 1;
        }
    }

    struct TagInfo {
        line_idx: usize,
        name_len: usize,
    }

    let mut groups: Vec<Vec<TagInfo>> = Vec::new();
    let mut current_group: Vec<TagInfo> = Vec::new();

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

        if let Some(after_tag) = content.strip_prefix(tag_keyword) {
            let name_len = tag_name_end(after_tag);

            current_group.push(TagInfo {
                line_idx: i,
                name_len,
            });
        }
        i += 1;
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }

    for group in &groups {
        let max_name_len = group.iter().map(|t| t.name_len).max().unwrap_or(0);

        let should_pad_names = group.len() >= 2;

        for tag_info in group {
            let line = &lines[tag_info.line_idx];
            let indent: String = line
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();

            let trimmed = line.trim();
            let content = trimmed.strip_prefix("* ").unwrap_or(trimmed);

            let after_tag = &content[tag_keyword.len()..];
            let name_end = tag_name_end(after_tag);
            let name = &after_tag[..name_end];
            let desc = if name_end < after_tag.len() {
                after_tag[name_end..].trim_start()
            } else {
                ""
            };

            let padding = if should_pad_names {
                max_name_len - tag_info.name_len
            } else {
                0
            };
            if desc.is_empty() {
                lines[tag_info.line_idx] = format!("{}* {}{}", indent, tag_keyword, name);
            } else {
                lines[tag_info.line_idx] = format!(
                    "{}* {}{}{} {}",
                    indent,
                    tag_keyword,
                    name,
                    " ".repeat(padding),
                    desc,
                );
            }

            let desc_col = tag_keyword.len() + max_name_len + 1;
            let cont_indent = " ".repeat(desc_col);
            let mut j = tag_info.line_idx + 1;
            while j < lines.len() {
                let jt = lines[j].trim();
                if jt == "*/" || jt == "**/" || jt == "*" {
                    break;
                }
                let jc = jt
                    .strip_prefix("* ")
                    .or_else(|| {
                        if jt == "*" {
                            Some("")
                        } else {
                            jt.strip_prefix("*")
                        }
                    })
                    .unwrap_or(jt);

                if jc.starts_with('@') {
                    break;
                }
                let j_indent: String = lines[j]
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                let cont_text = jc.trim();
                if !cont_text.is_empty() {
                    lines[j] = format!("{}* {}{}", j_indent, cont_indent, cont_text);
                }
                j += 1;
            }
        }
    }
}
