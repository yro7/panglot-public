/// Escapes the 5 HTML-significant characters to prevent XSS.
pub fn escape_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#x27;"),
            _ => output.push(c),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_script_tag() {
        assert_eq!(
            escape_html("<script>alert('xss')</script>"),
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
    }

    #[test]
    fn preserves_plain_text() {
        assert_eq!(escape_html("hello world"), "hello world");
    }

    #[test]
    fn escapes_all_special_chars() {
        assert_eq!(
            escape_html("a & b < c > d \" e ' f"),
            "a &amp; b &lt; c &gt; d &quot; e &#x27; f"
        );
    }

    #[test]
    fn empty_string() {
        assert_eq!(escape_html(""), "");
    }
}
