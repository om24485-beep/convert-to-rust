//! Small text utilities used by the extractors.

/// Maps byte offsets to 1-based line numbers.
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        LineIndex { starts }
    }

    pub fn line_of(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }
}

/// Given the offset of a signature, find the byte offset just past the matching
/// closing brace of the first `{` that follows. Skips strings and comments.
pub fn brace_block_end(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = from;
    // Find the opening brace (stop at `;` which means a declaration only).
    while i < bytes.len() {
        match bytes[i] {
            b'{' => break,
            b';' => return None,
            _ => i += 1,
        }
    }
    if i >= bytes.len() {
        return None;
    }
    let mut depth = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'"' | b'\'' | b'`' => {
                let quote = c;
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    // Only template literals span lines; stop an unterminated
                    // quote at the end of the line instead of eating the file.
                    if bytes[i] == b'\n' && quote != b'`' {
                        break;
                    }
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Indentation-based block end (Python, Ruby). `line` is 1-based line of the
/// header; returns the 1-based last line of the block.
pub fn indent_block_end(lines: &[&str], line: usize) -> usize {
    if line == 0 || line > lines.len() {
        return line;
    }
    let base = indent_of(lines[line - 1]);
    let mut last = line;
    // Skip continuation lines of a multi-line signature.
    let mut idx = line;
    while idx < lines.len() {
        let l = lines[idx];
        let trimmed = l.trim();
        if trimmed.is_empty() {
            idx += 1;
            continue;
        }
        let ind = indent_of(l);
        if ind <= base {
            // Ruby: include the closing `end` at the same indentation.
            if trimmed == "end" {
                last = idx + 1;
            }
            break;
        }
        last = idx + 1;
        idx += 1;
    }
    last
}

pub fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// Split a parameter list on top-level commas (ignores commas inside
/// brackets, parens, braces and angle brackets).
pub fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut cur = String::new();
    let mut in_str: Option<char> = None;
    for c in s.chars() {
        if let Some(q) = in_str {
            cur.push(c);
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '"' | '\'' | '`' => {
                in_str = Some(c);
                cur.push(c);
            }
            '(' | '[' | '{' | '<' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' | '}' | '>' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth <= 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Collapse whitespace (including newlines) to single spaces.
pub fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Count non-blank, non-comment-only lines.
pub fn code_lines(text: &str) -> usize {
    let mut in_block = false;
    text.lines()
        .filter(|l| {
            let t = l.trim();
            if in_block {
                if t.contains("*/") {
                    in_block = false;
                }
                return false;
            }
            if t.starts_with("/*") {
                in_block = !t.contains("*/");
                return false;
            }
            !(t.is_empty()
                || t.starts_with("//")
                || t.starts_with('#') && !t.starts_with("#include") && !t.starts_with("#define")
                || t.starts_with("--")
                || t.starts_with('*'))
        })
        .count()
}

/// Return lines `[start, end]` (1-based, inclusive) of `text`.
pub fn slice_lines(text: &str, start: usize, end: usize) -> String {
    text.lines()
        .skip(start.saturating_sub(1))
        .take(end.saturating_sub(start) + 1)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_nested() {
        assert_eq!(
            split_top_level("a: Map<string, number>, b = f(1, 2), c"),
            vec!["a: Map<string, number>", "b = f(1, 2)", "c"]
        );
    }

    #[test]
    fn finds_brace_end() {
        let src = "function f() { if (x) { return \"}\"; } }\nnext";
        let end = brace_block_end(src, 0).unwrap();
        assert_eq!(&src[end..], "\nnext");
    }

    #[test]
    fn python_block() {
        let src = "def f():\n    a = 1\n\n    return a\nx = 2";
        let lines: Vec<&str> = src.lines().collect();
        assert_eq!(indent_block_end(&lines, 1), 4);
    }

    #[test]
    fn line_index() {
        let idx = LineIndex::new("a\nb\nc");
        assert_eq!(idx.line_of(0), 1);
        assert_eq!(idx.line_of(2), 2);
        assert_eq!(idx.line_of(4), 3);
    }
}
