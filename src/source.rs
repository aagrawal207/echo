use std::fs;
use std::io::{self, Cursor, Read};
use std::path::Path;
use std::time::Duration;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use readability::extractor;
use url::Url;

const USER_AGENT: &str = concat!("echo/", env!("CARGO_PKG_VERSION"));
const FETCH_TIMEOUT_SECS: u64 = 20;
const MAX_FETCH_BYTES: u64 = 8 * 1024 * 1024;

pub fn load(path: Option<&str>) -> io::Result<String> {
    match path {
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
        Some(p) if is_url(p) => fetch_article(p),
        Some(p) => {
            let raw = fs::read_to_string(p)?;
            Ok(if is_markdown(p) {
                markdown_to_text(&raw)
            } else if is_html(p) {
                html_to_text(&raw, local_file_url(p).as_ref())
            } else {
                raw
            })
        }
    }
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown"))
}

fn is_html(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "html" | "htm" | "xhtml"))
}

fn local_file_url(path: &str) -> Option<Url> {
    Url::from_file_path(fs::canonicalize(path).ok()?).ok()
}

fn fetch_article(url: &str) -> io::Result<String> {
    let parsed = Url::parse(url)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("bad url: {e}")))?;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(FETCH_TIMEOUT_SECS)))
        .user_agent(USER_AGENT)
        .build()
        .into();

    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| io::Error::other(format!("fetching {url}: {e}")))?;

    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_FETCH_BYTES)
        .read_to_string()
        .map_err(|e| io::Error::other(format!("reading body from {url}: {e}")))?;

    Ok(html_to_text(&body, Some(&parsed)))
}

fn html_to_text(html: &str, base_url: Option<&Url>) -> String {
    let fallback = Url::parse("https://example.invalid/").expect("static url");
    let url = base_url.unwrap_or(&fallback);
    let mut cursor = Cursor::new(html.as_bytes().to_vec());
    match extractor::extract(&mut cursor, url) {
        Ok(product) => product.text,
        Err(_) => strip_tags_fallback(html),
    }
}

fn strip_tags_fallback(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut buf_lower = String::new();
    for c in html.chars() {
        if in_tag {
            buf_lower.push(c.to_ascii_lowercase());
            if c == '>' {
                if buf_lower.starts_with("script") {
                    in_script = true;
                }
                if buf_lower.starts_with("/script") {
                    in_script = false;
                }
                in_tag = false;
                buf_lower.clear();
            }
        } else if c == '<' {
            in_tag = true;
            buf_lower.clear();
        } else if !in_script {
            out.push(c);
        }
    }
    out
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

    #[test]
    fn detects_urls() {
        assert!(is_url("https://example.com"));
        assert!(is_url("http://example.com/page"));
        assert!(!is_url("example.com"));
        assert!(!is_url("file.html"));
        assert!(!is_url("/abs/path"));
    }

    #[test]
    fn detects_html_extension() {
        assert!(is_html("page.html"));
        assert!(is_html("page.HTM"));
        assert!(is_html("x.xhtml"));
        assert!(!is_html("x.md"));
    }

    #[test]
    fn fallback_strips_tags_and_scripts() {
        let html = "<p>Hello <b>world</b></p><script>alert('x')</script><p>Bye</p>";
        let out = strip_tags_fallback(html);
        assert!(out.contains("Hello"));
        assert!(out.contains("world"));
        assert!(out.contains("Bye"));
        assert!(!out.contains("alert"));
    }

    #[test]
    fn extracts_article_text_from_html() {
        // Readability needs substantial content to score as an article.
        let html = r#"<!DOCTYPE html>
<html><body>
<nav>Home About Contact Us</nav>
<article>
  <h1>Article Title</h1>
  <p>The quick brown fox jumps over the lazy dog. This paragraph has
  enough prose for readability's heuristic to classify it as the main
  content body of the document rather than boilerplate chrome.</p>
  <p>A second paragraph reinforces that judgement by adding more
  human-readable text. Readability looks at text density and tag
  ratios to make this call.</p>
</article>
<footer>Copyright 2026 — Privacy — Terms</footer>
<script>analytics();</script>
</body></html>"#;
        let url = Url::parse("http://example.com/article").unwrap();
        let out = html_to_text(html, Some(&url));
        assert!(out.contains("quick brown fox"));
        assert!(out.contains("second paragraph"));
        assert!(!out.contains("analytics"));
    }
}
