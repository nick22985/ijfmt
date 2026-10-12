pub(super) fn starts_with_todo(line: &str) -> bool {
    let t = line.trim_start();
    t.len() >= 4
        && t[..4].eq_ignore_ascii_case("todo")
        && t[4..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric())
}

pub(super) fn tag_name_end(after_tag: &str) -> usize {
    after_tag.find([' ', '\t']).unwrap_or(after_tag.len())
}

pub(super) fn split_tag_prefix(content: &str) -> (String, &str) {
    let named_tags = ["@param ", "@throws ", "@exception ", "@serialField "];
    for tag in &named_tags {
        if let Some(after_tag) = content.strip_prefix(tag) {
            if let Some(space_pos) = after_tag.find([' ', '\t']) {
                let prefix = &content[..tag.len() + space_pos + 1];
                let desc = &content[tag.len() + space_pos + 1..];
                return (prefix.to_string(), desc);
            } else {
                return (content.to_string() + " ", "");
            }
        }
    }
    if let Some(space_pos) = content.find(' ') {
        let prefix = &content[..space_pos + 1];
        let desc = &content[space_pos + 1..];
        (prefix.to_string(), desc)
    } else {
        (content.to_string() + " ", "")
    }
}

fn tokenize_for_wrap(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c == '{' && chars.clone().nth(1) == Some('@') {
            let attached = if !current.is_empty() && !current.ends_with(char::is_whitespace) {
                true
            } else if current.is_empty() {
                tokens
                    .last()
                    .is_some_and(|t: &String| !t.ends_with(char::is_whitespace))
            } else {
                false
            };
            if !current.is_empty() {
                let leftover = split_preserving_double_spaces(&current, &mut tokens);
                if !leftover.is_empty()
                    && let Some(last) = tokens.last_mut()
                {
                    last.push_str(&leftover);
                }
                current.clear();
            }
            let mut brace_token = String::new();
            let mut depth = 0;
            for bc in chars.by_ref() {
                brace_token.push(bc);
                if bc == '{' {
                    depth += 1;
                }
                if bc == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            while let Some(&next_c) = chars.peek() {
                if next_c.is_whitespace() || (next_c == '{' && chars.clone().nth(1) == Some('@')) {
                    break;
                }
                brace_token.push(chars.next().unwrap());
            }
            {
                let mut run = 0usize;
                let mut look = chars.clone();
                while let Some(&sc) = look.peek() {
                    if sc == ' ' || sc == '\t' {
                        run += 1;
                        look.next();
                    } else {
                        break;
                    }
                }
                if run >= 2 {
                    brace_token.push_str(&" ".repeat(run - 1));
                }
            }
            if attached && !tokens.is_empty() {
                let last = tokens.last_mut().unwrap();
                last.push_str(&brace_token);
            } else {
                tokens.push(brace_token);
            }
        } else {
            current.push(chars.next().unwrap());
        }
    }
    if !current.is_empty() {
        let leftover = split_preserving_double_spaces(&current, &mut tokens);
        if !leftover.is_empty()
            && let Some(last) = tokens.last_mut()
        {
            last.push_str(&leftover);
        }
    }
    tokens
}

fn split_preserving_double_spaces(text: &str, tokens: &mut Vec<String>) -> String {
    let mut start = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut pending = String::new();

    while start < len {
        while start < len && (bytes[start] == b' ' || bytes[start] == b'\t') {
            start += 1;
        }
        if start >= len {
            break;
        }

        let word_start = start;
        while start < len && bytes[start] != b' ' && bytes[start] != b'\t' {
            start += 1;
        }
        let word = &text[word_start..start];

        let space_start = start;
        while start < len && (bytes[start] == b' ' || bytes[start] == b'\t') {
            start += 1;
        }
        let space_count = start - space_start;

        if pending.is_empty() {
            tokens.push(word.to_string());
        } else {
            tokens.push(format!("{}{}", pending, word));
            pending.clear();
        }

        if space_count >= 2 {
            pending = " ".repeat(space_count - 1);
        }
    }
    pending
}

fn compute_wrap_position(line: &[char], width: usize) -> Option<usize> {
    if line.len() < width {
        return None;
    }
    let mut preferred: Option<usize> = None;
    let mut backup: Option<usize> = None;
    let mut tag_brace_balance: i32 = 0;
    let mut i = 0usize;
    while i < line.len() && (i <= width || backup.is_none()) {
        let c = line[i];
        if tag_brace_balance > 0 {
            if c == '{' {
                tag_brace_balance += 1;
            } else if c == '}' {
                tag_brace_balance -= 1;
            }
        } else if c == '@' && i > 0 && line[i - 1] == '{' {
            tag_brace_balance += 1;
        } else if c == ' ' {
            preferred = Some(i);
        }
        if c == ' ' {
            backup = Some(i);
        }
        i += 1;
    }
    preferred.filter(|&i| i > 0).or(backup.filter(|&i| i > 0))
}

fn split_inline_tag(token: &str, first_width: usize, rest_width: usize) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut width = first_width;
    loop {
        let rem = &chars[start..];
        let wp = match compute_wrap_position(rem, width) {
            Some(wp) if wp + 1 < rem.len() => wp,
            _ => {
                parts.push(rem.iter().collect());
                break;
            }
        };
        parts.push(rem[..wp].iter().collect());
        start += wp + 1;
        width = rest_width;
    }
    parts
}

fn split_inline_tag_at_comma(token: &str, first_width: usize) -> Option<Vec<String>> {
    let mut best: Option<usize> = None;
    for (ci, (bi, ch)) in token.char_indices().enumerate() {
        if ci > first_width {
            break;
        }
        if ch == ' ' {
            best = Some(bi);
        }
    }
    best.map(|bi| vec![token[..bi].to_string(), token[bi + 1..].to_string()])
}

fn carry_run_prefix(token: &str, line_len: usize, width: usize) -> String {
    let p = token.chars().take_while(|c| *c == ' ').count();
    if p == 0 {
        return token.to_string();
    }
    let keep = (line_len + p).saturating_sub(width).min(p);
    format!("{}{}", " ".repeat(keep), &token[p..])
}

pub(super) fn wrap_text_tokens(text: &str, first_width: usize, rest_width: usize) -> Vec<String> {
    let tokens = tokenize_for_wrap(text);
    if tokens.is_empty() {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let mut current_width = first_width;
    let token_count = tokens.len();

    for (idx, token) in tokens.iter().enumerate() {
        let is_trailing_punctuation =
            matches!(token.as_str(), ")" | "]" | "." | ";" | "!") && current_line.ends_with('}');

        if current_line.is_empty() {
            let tok_len = token.chars().count();
            let is_last = idx + 1 == token_count;
            let wrap_at_boundary_inline = first_width == rest_width || current_width == rest_width;
            let boundary_split = token.starts_with("{@")
                && tok_len == current_width
                && is_last
                && wrap_at_boundary_inline
                && split_inline_tag_at_comma(token, current_width).is_some();
            if tok_len > current_width || boundary_split {
                let parts = split_inline_tag(token, current_width, rest_width);
                for part in &parts[..parts.len() - 1] {
                    lines.push(part.clone());
                }
                current_line = parts[parts.len() - 1].clone();
                if parts.len() > 1 {
                    current_width = rest_width;
                }
            } else {
                current_line = token.clone();
            }
        } else if is_trailing_punctuation {
            current_line.push_str(token);
        } else {
            let new_len = current_line.chars().count() + 1 + token.chars().count();
            let is_last = idx + 1 == token_count;

            let at_boundary = new_len == current_width && is_last;
            let wrap_at_boundary = first_width == rest_width || current_width == rest_width;
            let fits = if at_boundary && wrap_at_boundary {
                false
            } else {
                new_len <= current_width
            };

            if fits {
                current_line.push(' ');
                current_line.push_str(token);
            } else {
                let closed_len = current_line.chars().count();
                let closed_width = current_width;
                lines.push(current_line);
                current_width = rest_width;
                let tok_len = token.chars().count();
                let boundary_split = token.starts_with("{@")
                    && tok_len == current_width
                    && is_last
                    && split_inline_tag_at_comma(token, current_width).is_some();
                if tok_len > current_width || boundary_split {
                    let parts = split_inline_tag(token, current_width, current_width);
                    for part in &parts[..parts.len() - 1] {
                        lines.push(part.clone());
                    }
                    current_line = parts[parts.len() - 1].clone();
                } else {
                    current_line = carry_run_prefix(token, closed_len, closed_width);
                }
            }
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line.trim_start().to_string());
    }

    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}
