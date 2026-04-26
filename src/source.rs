use std::fs;
use std::io::{self, Read};
use std::path::Path;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

pub fn load(path: Option<&str>) -> io::Result<String> {
    match path {
        Some(p) => {
            let raw = fs::read_to_string(p)?;
            Ok(if is_markdown(p) {
                markdown_to_text(&raw)
            } else {
                raw
            })
        }
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown"))
}

fn markdown_to_text(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut skip_depth: u32 = 0;

    for event in Parser::new(md) {
        match event {
            Event::Start(Tag::CodeBlock(_)) => skip_depth += 1,
            Event::End(TagEnd::CodeBlock) => skip_depth = skip_depth.saturating_sub(1),
            _ if skip_depth > 0 => {}

            Event::Text(t) | Event::Code(t) => out.push_str(&t),

            Event::SoftBreak | Event::HardBreak => out.push(' '),

            Event::End(TagEnd::Paragraph)
            | Event::End(TagEnd::Heading(_))
            | Event::End(TagEnd::Item)
            | Event::End(TagEnd::BlockQuote(_)) => out.push('\n'),

            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_headings_and_emphasis() {
        let md = "# Title\n\nSome *bold* text with `code`.\n";
        let out = markdown_to_text(md);
        assert!(out.contains("Title"));
        assert!(out.contains("Some bold text with code."));
        assert!(!out.contains('#'));
        assert!(!out.contains('*'));
    }

    #[test]
    fn drops_code_blocks() {
        let md = "Before.\n\n```rust\nfn main() {}\n```\n\nAfter.\n";
        let out = markdown_to_text(md);
        assert!(out.contains("Before"));
        assert!(out.contains("After"));
        assert!(!out.contains("fn main"));
    }

    #[test]
    fn inline_code_kept() {
        assert!(markdown_to_text("Use `foo` here.").contains("Use foo here."));
    }

    #[test]
    fn detects_markdown_extension() {
        assert!(is_markdown("notes.md"));
        assert!(is_markdown("NOTES.MD"));
        assert!(is_markdown("a/b/c.markdown"));
        assert!(!is_markdown("plain.txt"));
        assert!(!is_markdown("no_ext"));
    }
}
