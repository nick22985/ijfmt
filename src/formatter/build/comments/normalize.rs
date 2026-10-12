use super::text::starts_with_todo;

pub(super) fn expand_single_line_javadoc(raw: &str) -> String {
    let indent: String = raw
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let text = &raw[indent.len()..];
    let b = text.as_bytes();

    let header_end = {
        let mut i = 1;
        while i < b.len() && b[i] == b'*' {
            i += 1;
        }
        i
    };
    let footer_start = {
        let mut i = b.len() as isize - 2;
        while i >= 0 && b[i as usize] == b'*' {
            i -= 1;
        }
        i
    };
    let (content, footer) = if footer_start >= header_end as isize {
        let fs = footer_start as usize;
        (text[header_end..fs + 1].trim(), &text[fs + 1..])
    } else {
        ("", "*/")
    };

    if content.is_empty() {
        format!("{}/**\n{} *\n{} {}", indent, indent, indent, footer)
    } else {
        format!(
            "{}/**\n{} * {}\n{} {}",
            indent, indent, content, indent, footer
        )
    }
}

pub(super) fn insert_p_tags(lines: &mut [String]) {
    let snapshot = lines.to_vec();
    let mut seen_content = false;
    let mut in_pre = false;

    for i in 0..lines.len() {
        let trimmed = snapshot[i].trim();

        let line_content = trimmed
            .strip_prefix("* ")
            .or_else(|| trimmed.strip_prefix("*"))
            .unwrap_or(trimmed);

        if line_content.contains("<pre>") || line_content.contains("<pre ") {
            in_pre = true;
        }
        if line_content.contains("</pre>") {
            in_pre = false;
        }

        if trimmed != "*" {
            if trimmed != "/**" && trimmed != "*/" && trimmed != "**/" {
                seen_content = true;
            }
            continue;
        }

        if in_pre {
            continue;
        }

        if !seen_content {
            continue;
        }

        let mut next_content: Option<&str> = None;
        for line in snapshot.iter().skip(i + 1) {
            let nt = line.trim();
            if nt == "*" || nt.is_empty() {
                continue;
            }
            next_content = Some(nt);
            break;
        }

        if let Some(nc) = next_content {
            let content = nc
                .strip_prefix("* ")
                .or_else(|| nc.strip_prefix("*"))
                .unwrap_or(nc)
                .trim_start();

            if !content.starts_with('@')
                && !content.starts_with("<p>")
                && !content.starts_with("<p/>")
                && !content.starts_with('/')
                && !content.starts_with('<')
            {
                let indent: String = lines[i]
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                lines[i] = format!("{}* <p>", indent);
            }
        }
    }
}

pub(super) fn normalize_extra_spaces_after_star(lines: &mut [String]) {
    let keeper_tags = ["ol", "ul", "table", "dl", "div"];
    let mut keep_indents = false;
    let mut pre_depth = 0i32;
    let mut in_snippet = false;
    let mut in_todo = false;

    for line in lines.iter_mut() {
        let line_content: String = {
            let trimmed_line = line.trim();
            trimmed_line
                .strip_prefix("* ")
                .or_else(|| trimmed_line.strip_prefix("*"))
                .unwrap_or(trimmed_line)
                .to_string()
        };

        if in_snippet {
            let trimmed_content = line_content.trim();
            if trimmed_content == "}" || trimmed_content.ends_with('}') {
                in_snippet = false;
                if trimmed_content == "}"
                    && let Some(star_pos) = line.find('*')
                {
                    let prefix = &line[..star_pos];
                    *line = format!("{}*}}", prefix);
                }
            }
        }

        if !keep_indents {
            let lc = line_content.trim();
            if let Some(lc) = lc.strip_prefix('<') {
                let name: String = lc
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if keeper_tags.contains(&name.as_str()) {
                    keep_indents = true;
                }
            }
        }
        if line_content.contains("<pre>") || line_content.contains("<pre ") {
            pre_depth += 1;
        }

        let content_trimmed = line_content.trim();
        let is_p_tag = content_trimmed == "<p>" || content_trimmed == "<p/>";

        if content_trimmed.starts_with('@') {
            in_todo = false;
        } else if !in_todo && starts_with_todo(content_trimmed) {
            in_todo = true;
        }

        if ((!keep_indents && pre_depth == 0 && !in_todo) || is_p_tag)
            && !in_snippet
            && let Some(star_pos) = line.find("* ")
        {
            let after_star = &line[star_pos + 2..];
            if after_star.starts_with(' ') || after_star.starts_with('\t') {
                let content = after_star.trim_start();
                if !content.is_empty() {
                    let prefix = &line[..star_pos];
                    *line = format!("{}* {}", prefix, content);
                }
            }
            let after_star2 = &line[star_pos + 2..];
            if after_star2.starts_with('@')
                && let Some(space_after_tag) = after_star2.find(' ')
            {
                let after_tag = &after_star2[space_after_tag..];
                if after_tag.starts_with("  ") {
                    let tag_part = &after_star2[..space_after_tag];
                    let rest = after_tag.trim_start();
                    if !rest.is_empty() {
                        let prefix = &line[..star_pos];
                        *line = format!("{}* {} {}", prefix, tag_part, rest);
                    }
                }
            }
        }

        if !in_snippet && line_content.contains("{@snippet") && line_content.contains(':') {
            in_snippet = true;
        }

        pre_depth -= line_content.matches("</pre>").count() as i32;
        if pre_depth < 0 {
            pre_depth = 0;
        }
    }
}

pub(super) fn normalize_see_commas(lines: &mut [String]) {
    for line in lines.iter_mut() {
        if !line.contains("{@link") {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut out: Vec<char> = Vec::with_capacity(chars.len() + 4);
        let mut i = 0usize;
        let mut in_link = false;
        let mut in_paren = false;
        while i < chars.len() {
            let c = chars[i];
            if !in_link && c == '{' && chars[i..].iter().collect::<String>().starts_with("{@link") {
                in_link = true;
            }
            out.push(c);
            if in_link {
                match c {
                    '(' => in_paren = true,
                    ')' => in_paren = false,
                    '}' => {
                        in_link = false;
                        in_paren = false;
                    }
                    ',' if in_paren => {
                        let mut j = i + 1;
                        while j < chars.len() && chars[j] == ' ' {
                            j += 1;
                        }
                        out.push(' ');
                        i = j - 1;
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        let new_line: String = out.into_iter().collect();
        if &new_line != line {
            *line = new_line;
        }
    }
    for line in lines.iter_mut() {
        let trimmed = line.trim();
        let content = trimmed
            .strip_prefix("* ")
            .or_else(|| trimmed.strip_prefix("*"))
            .unwrap_or(trimmed)
            .trim_start();
        if content.starts_with("@see ")
            && let Some(paren_start) = line.find('(')
            && let Some(paren_end) = line[paren_start..].find(')')
        {
            let inner = &line[paren_start + 1..paren_start + paren_end];
            if inner.contains(',') {
                let mut normalized = String::new();
                let mut chars = inner.chars().peekable();
                while let Some(c) = chars.next() {
                    normalized.push(c);
                    if c == ',' {
                        while chars.peek() == Some(&' ') {
                            chars.next();
                        }
                        normalized.push(' ');
                    }
                }
                let new_line = format!(
                    "{}({}){}",
                    &line[..paren_start],
                    normalized,
                    &line[paren_start + paren_end + 1..]
                );
                *line = new_line;
            }
        }
    }
}

pub(super) fn strip_space_after_broken_doc_ref(lines: &mut [String]) {
    let mut depth = 0i32;
    for i in 0..lines.len() {
        let line = lines[i].clone();
        let ends_open_paren = depth_after_doc_ref_scan(&line, &mut depth);
        if depth > 0 && ends_open_paren && i + 1 < lines.len() {
            let next = lines[i + 1].trim_start();
            if let Some(rest) = next.strip_prefix("* ")
                && !rest.is_empty()
                && !rest.starts_with('*')
            {
                let indent: String = lines[i + 1]
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                lines[i + 1] = format!("{indent}*{rest}");
            }
        }
    }
}

fn depth_after_doc_ref_scan(line: &str, depth: &mut i32) -> bool {
    let b: Vec<char> = line.chars().collect();
    let mut last_open = false;
    let mut i = 0;
    while i < b.len() {
        if *depth > 0 {
            match b[i] {
                '(' => {
                    *depth += 1;
                    last_open = true;
                }
                ')' => {
                    *depth -= 1;
                    last_open = false;
                }
                c if c == ' ' || c == '\t' => {}
                '}' | '@' => {
                    *depth = 0;
                    last_open = false;
                }
                _ => last_open = false,
            }
            i += 1;
            continue;
        }
        if b[i] != '#' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && (b[j].is_alphanumeric() || b[j] == '_' || b[j] == '$') {
            j += 1;
        }
        if j == i + 1 || j >= b.len() || b[j] != '(' {
            i = j.max(i + 1);
            continue;
        }
        *depth = 1;
        last_open = true;
        i = j + 1;
    }
    last_open && *depth > 0
}

pub(crate) fn normalize_doc_ref_commas(text: &str) -> String {
    let b: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != '#' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && (b[j].is_alphanumeric() || b[j] == '_' || b[j] == '$') {
            j += 1;
        }
        if j == i + 1 || j >= b.len() || b[j] != '(' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let mut k = j + 1;
        while k < b.len() && b[k] != ')' && b[k] != '\n' {
            k += 1;
        }
        if k >= b.len() || b[k] != ')' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        out.extend(&b[i..=j]);
        let mut m = j + 1;
        while m < k && b[m] == ' ' {
            m += 1;
        }
        while m < k {
            if b[m] == ',' {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(',');
                let mut n = m + 1;
                while n < k && b[n] == ' ' {
                    n += 1;
                }
                if n < k {
                    out.push(' ');
                }
                m = n;
            } else {
                out.push(b[m]);
                m += 1;
            }
        }
        while out.ends_with(' ') {
            out.pop();
        }
        out.push(')');
        i = k + 1;
    }
    out
}
