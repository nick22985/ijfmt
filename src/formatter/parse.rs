pub(super) struct ParsedJava {
    pub tree: tree_sitter::Tree,
    pub depth: usize,
}

pub(super) fn parse(source: &str) -> anyhow::Result<ParsedJava> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into())?;

    let repaired = repair_annotated_varargs(source);
    let parse_source: &str = repaired.as_deref().unwrap_or(source);

    let mut tree = parser
        .parse(parse_source, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter failed to parse"))?;

    if tree.root_node().has_error()
        && let Some(fixed) = repair_case_pattern_modifiers(parse_source)
        && let Some(fixed_tree) = parser.parse(&fixed, None)
        && error_count(fixed_tree.root_node()) < error_count(tree.root_node())
    {
        tree = fixed_tree;
    }

    let depth = syntax_depth(tree.root_node())?;
    Ok(ParsedJava { tree, depth })
}

fn repair_annotated_varargs(source: &str) -> Option<String> {
    let b = source.as_bytes();
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
    let is_blank = |c: u8| c == b' ' || c == b'\t';
    let mut fills: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i + 3 <= b.len() {
        if &b[i..i + 3] != b"..." {
            i += 1;
            continue;
        }
        let mut p = i;
        let mut here: Vec<usize> = Vec::new();
        let mut first = true;
        loop {
            let blank_end = p;
            while p > 0 && is_blank(b[p - 1]) {
                p -= 1;
            }
            if p == blank_end || p == 0 {
                break;
            }
            if !first {
                here.extend(p..blank_end);
            }
            first = false;
            let name_end = p;
            while p > 0 && (is_word(b[p - 1]) || b[p - 1] == b'.') {
                p -= 1;
            }
            if p == name_end {
                here.clear();
                break;
            }
            if p > 0 && b[p - 1] == b'@' {
                p -= 1;
                here.push(p);
                continue;
            }
            fills.append(&mut here);
            break;
        }
        i += 3;
    }
    if fills.is_empty() {
        return None;
    }
    let mut out = b.to_vec();
    for f in fills {
        out[f] = b'_';
    }
    String::from_utf8(out).ok()
}

fn repair_case_pattern_modifiers(source: &str) -> Option<String> {
    let b = source.as_bytes();
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
    let mut blanks: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i + 4 <= b.len() {
        if &b[i..i + 4] != b"case" {
            i += 1;
            continue;
        }
        if i > 0 && (is_word(b[i - 1]) || b[i - 1] == b'.' || b[i - 1] == b'@') {
            i += 1;
            continue;
        }
        if i + 4 < b.len() && is_word(b[i + 4]) {
            i += 1;
            continue;
        }
        let mut p = i + 4;
        loop {
            let blank_start = p;
            while p < b.len() && (b[p] == b' ' || b[p] == b'\t') {
                p += 1;
            }
            if p == blank_start || p >= b.len() {
                break;
            }
            if b[p] == b'@' {
                let start = p;
                p += 1;
                let name_start = p;
                while p < b.len() && (is_word(b[p]) || b[p] == b'.') {
                    p += 1;
                }
                if p == name_start {
                    break;
                }
                if p < b.len() && b[p] == b'(' {
                    let mut depth = 0usize;
                    let mut closed = false;
                    while p < b.len() {
                        match b[p] {
                            b'(' => depth += 1,
                            b')' => {
                                depth -= 1;
                                if depth == 0 {
                                    p += 1;
                                    closed = true;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        p += 1;
                    }
                    if !closed {
                        break;
                    }
                }
                blanks.push((start, p));
                continue;
            }
            if p + 5 <= b.len()
                && &b[p..p + 5] == b"final"
                && (p + 5 == b.len() || !is_word(b[p + 5]))
            {
                blanks.push((p, p + 5));
                p += 5;
                continue;
            }
            break;
        }
        i += 4;
    }
    if blanks.is_empty() {
        return None;
    }
    let mut out = b.to_vec();
    for (start, end) in blanks {
        for byte in &mut out[start..end] {
            *byte = b' ';
        }
    }
    String::from_utf8(out).ok()
}

fn error_count(node: tree_sitter::Node) -> usize {
    let mut cursor = node.walk();
    let mut count = 0;
    loop {
        let node = cursor.node();
        count += usize::from(node.is_error() || node.is_missing());
        if node.has_error() && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return count;
            }
        }
    }
}

fn syntax_depth(root: tree_sitter::Node) -> anyhow::Result<usize> {
    const MAX_DEPTH: usize = 512;
    let mut cursor = root.walk();
    let mut depth = 0;
    let mut maximum = 0;
    loop {
        if cursor.goto_first_child() {
            depth += 1;
            anyhow::ensure!(
                depth <= MAX_DEPTH,
                "Java syntax nesting exceeds supported depth {MAX_DEPTH}"
            );
            maximum = maximum.max(depth);
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(maximum);
            }
            depth -= 1;
        }
    }
}
