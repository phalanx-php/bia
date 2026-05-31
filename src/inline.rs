pub fn wrap_inline_code(code: &str) -> String {
    let code = expand_bare_vars(code.trim());
    let is_expression = !code.contains(';') && !code.contains('{') && !contains_assignment(&code);

    let body = if is_expression {
        format!("$__r = ({code});\nif ($__r !== null) {{ dory()->dump($__r); }}\nreturn 0;")
    } else {
        let stmts = if code.ends_with(';') || code.ends_with('}') || code.ends_with("?>") {
            code.clone()
        } else {
            format!("{code};")
        };
        format!("{stmts}\nreturn 0;")
    };

    format!("<?php declare(strict_types=1);\n{body}\n")
}

fn contains_assignment(input: &str) -> bool {
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\'' || bytes[i] == b'"' {
            i = skip_quoted(bytes, i);
            continue;
        }

        if bytes[i] != b'=' {
            i += 1;
            continue;
        }

        let prev = i.checked_sub(1).map(|idx| bytes[idx]);
        let next = bytes.get(i + 1).copied();

        if matches!(prev, Some(b'=' | b'!' | b'<' | b'>' | b'-'))
            || matches!(next, Some(b'=' | b'>'))
        {
            i += 1;
            continue;
        }

        return true;
    }

    false
}

fn skip_quoted(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;

    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }

        if bytes[i] == quote {
            return i + 1;
        }

        i += 1;
    }

    i
}

fn expand_bare_vars(input: &str) -> String {
    const KEYWORDS: &[&str] = &["fn", "if", "do", "as", "or", "in", "dd", "fs"];

    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + 32);
    let mut i = 0;

    while i < len {
        let ch = bytes[i] as char;

        if ch == '\'' {
            out.push(ch);
            i += 1;

            while i < len {
                let c = bytes[i] as char;
                out.push(c);
                i += 1;

                if c == '\\' && i < len {
                    out.push(bytes[i] as char);
                    i += 1;
                } else if c == '\'' {
                    break;
                }
            }

            continue;
        }

        if ch == '"' {
            out.push(ch);
            i += 1;

            while i < len {
                let c = bytes[i] as char;
                out.push(c);
                i += 1;

                if c == '\\' && i < len {
                    out.push(bytes[i] as char);
                    i += 1;
                } else if c == '"' {
                    break;
                }
            }

            continue;
        }

        if ch == '$' {
            out.push(ch);
            i += 1;

            while i < len && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_') {
                out.push(bytes[i] as char);
                i += 1;
            }

            continue;
        }

        if ch.is_ascii_lowercase() {
            let start = i;

            if start > 0
                && ((bytes[start - 1] as char).is_ascii_alphanumeric() || bytes[start - 1] == b'_')
            {
                out.push(ch);
                i += 1;
                continue;
            }

            let mut end = start + 1;

            while end < len && (bytes[end] as char).is_ascii_lowercase() {
                end += 1;
            }

            let ident_len = end - start;

            if ident_len <= 2
                && (end >= len
                    || (!(bytes[end] as char).is_ascii_alphanumeric() && bytes[end] != b'_'))
            {
                let ident = &input[start..end];

                if !KEYWORDS.contains(&ident) {
                    out.push('$');
                }
            }

            out.push_str(&input[start..end]);

            i = end;
            continue;
        }

        out.push(ch);
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_expression_for_dumping() {
        let wrapped = wrap_inline_code("1 + 1");

        assert!(wrapped.contains("$__r = (1 + 1);"));
        assert!(wrapped.contains("dory()->dump($__r);"));
    }

    #[test]
    fn adds_semicolon_to_statement_body() {
        let wrapped = wrap_inline_code("x = 42");

        assert!(wrapped.contains("$x = 42;"));
        assert!(wrapped.contains("return 0;"));
    }

    #[test]
    fn preserves_existing_php_variables_and_strings() {
        let wrapped = wrap_inline_code(r#"$x = "hello y"; dump($x)"#);

        assert!(wrapped.contains(r#"$x = "hello y"; dump($x);"#));
    }

    #[test]
    fn preserves_short_keywords() {
        let wrapped = wrap_inline_code("array_map(fn(n) => n * 2, [1, 2])");

        assert!(wrapped.contains("fn($n) => $n * 2"));
    }

    #[test]
    fn preserves_comparison_expressions() {
        let wrapped = wrap_inline_code("x == 42");

        assert!(wrapped.contains("$__r = ($x == 42);"));
    }

    #[test]
    fn ignores_assignment_tokens_inside_strings() {
        let wrapped = wrap_inline_code(r#""x = 42""#);

        assert!(wrapped.contains(r#"$__r = ("x = 42");"#));
    }
}
