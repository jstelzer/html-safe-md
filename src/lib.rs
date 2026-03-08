//! # html-safe-md
//!
//! Convert untrusted HTML into safe, readable markdown.
//!
//! Built for email but useful anywhere you need to render HTML from an
//! untrusted source without a webview, remote fetches, or JavaScript execution.
//!
//! ## Pipeline
//!
//! ```text
//! untrusted HTML → prep_block_breaks → ammonia (allowlist) → html2md → post_process → safe string
//! ```
//!
//! ## Quick Start
//!
//! ```rust
//! // Sanitize HTML to markdown with default email-tuned config
//! let md = html_safe_md::sanitize_html("<p>Hello <strong>world</strong></p>");
//! assert!(md.contains("**world**"));
//!
//! // Render an email body (prefers plain text, falls back to sanitized HTML)
//! let md = html_safe_md::render_email(
//!     Some("Hey, just following up on our conversation from yesterday.\n\nLet me know."),
//!     Some("<p>HTML version</p>"),
//! );
//! assert!(md.contains("following up"));
//! ```

mod pipeline;

use std::collections::HashSet;

use pipeline::{clean_html, post_process_md, post_process_plain, prep_block_breaks};

/// Max HTML input size before truncation (512 KB).
const DEFAULT_MAX_HTML_BYTES: usize = 512 * 1024;

/// Max markdown output length in chars.
const DEFAULT_MAX_MD_CHARS: usize = 200_000;

/// URLs longer than this in markdown links get dropped (link text kept).
const MAX_URL_LEN: usize = 200;

/// Tags that produce meaningful markdown. Everything else is stripped.
/// Text content inside stripped tags is preserved — only the tags are removed.
const ALLOWED_TAGS: &[&str] = &[
    // Block content
    "p", "br", "hr", "blockquote", "pre",
    // Headings
    "h1", "h2", "h3", "h4", "h5", "h6",
    // Inline formatting
    "b", "strong", "i", "em", "code", "s", "del", "u", "small", "sub", "sup",
    // Lists
    "ul", "ol", "li",
    // Links (ammonia sanitizes href — no javascript: URIs)
    "a",
];

/// Configuration for the sanitizer.
#[derive(Debug, Clone)]
pub struct Config {
    /// Max HTML input size in bytes before truncation.
    pub max_html_bytes: usize,
    /// Max markdown output length in chars.
    pub max_md_chars: usize,
    /// Additional tags to allow beyond the default email set.
    pub extra_tags: HashSet<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_html_bytes: DEFAULT_MAX_HTML_BYTES,
            max_md_chars: DEFAULT_MAX_MD_CHARS,
            extra_tags: HashSet::new(),
        }
    }
}

/// Sanitize untrusted HTML and convert to markdown using the default config.
///
/// Strips all tags except semantic content tags (paragraphs, headings, lists,
/// links, inline formatting). No `<img>`, no `<table>`, no `<style>`, no
/// `<script>`. Text inside stripped tags is preserved.
///
/// # Examples
///
/// ```rust
/// let md = html_safe_md::sanitize_html("<p>Click <a href=\"https://example.com\">here</a></p>");
/// assert!(md.contains("[here](https://example.com)"));
/// ```
pub fn sanitize_html(html: &str) -> String {
    sanitize_html_with(html, &Config::default())
}

/// Sanitize untrusted HTML and convert to markdown with a custom config.
pub fn sanitize_html_with(html: &str, config: &Config) -> String {
    let html = &html[..html.len().min(config.max_html_bytes)];
    let prepped = prep_block_breaks(html);
    let clean = clean_html(&prepped, config);
    let md = html2md::parse_html(&clean);
    let mut md = post_process_md(&md);
    md.truncate(config.max_md_chars);
    md
}

/// Render an email body as markdown, preferring plain text when available.
///
/// Strategy:
/// 1. If `text_plain` looks like real content, return it as-is.
/// 2. If `text_plain` is junk (stub/tracker), fall through to HTML.
/// 3. Sanitize `text_html` via the ammonia → html2md pipeline.
/// 4. If only junk plain text exists (no HTML), return it anyway.
///
/// # Examples
///
/// ```rust
/// // Plain text preferred when it's real content
/// let md = html_safe_md::render_email(
///     Some("Hey,\n\nThis is a real email with enough content to not be junk.\n\nCheers"),
///     Some("<p>HTML version</p>"),
/// );
/// assert!(md.contains("real email"));
///
/// // Falls back to sanitized HTML when plain is a stub
/// let md = html_safe_md::render_email(
///     Some("View online"),
///     Some("<p>The <strong>actual</strong> newsletter content lives here.</p>"),
/// );
/// assert!(md.contains("actual"));
/// ```
pub fn render_email(text_plain: Option<&str>, text_html: Option<&str>) -> String {
    render_email_with(text_plain, text_html, &Config::default())
}

/// Render an email body as markdown with a custom config.
pub fn render_email_with(
    text_plain: Option<&str>,
    text_html: Option<&str>,
    config: &Config,
) -> String {
    if let Some(plain) = text_plain {
        if !is_junk_plain(plain) {
            return plain.to_string();
        }
    }

    if let Some(html) = text_html {
        return sanitize_html_with(html, config);
    }

    if let Some(plain) = text_plain {
        return plain.to_string();
    }

    "[No displayable content]".to_string()
}

/// Render an email body as plain text (no markdown formatting).
///
/// Prefers `text_plain` when available; falls back to sanitized HTML → text.
/// The HTML is cleaned through ammonia first (same as the markdown path) to
/// strip layout tables, styles, and scripts before converting to text.
pub fn render_email_plain(text_plain: Option<&str>, text_html: Option<&str>) -> String {
    if let Some(plain) = text_plain {
        return plain.to_string();
    }

    if let Some(html) = text_html {
        let prepped = prep_block_breaks(html);
        let clean = clean_html(&prepped, &Config::default());
        let text = html2text::from_read(clean.as_bytes(), 80).unwrap_or_default();
        return post_process_plain(&text);
    }

    "[No displayable content]".to_string()
}

/// Returns true if the plain-text part looks like a stub or tracking junk
/// rather than real email content.
///
/// Heuristics:
/// - Empty or whitespace-only
/// - Under 40 characters
/// - Two or fewer lines
pub fn is_junk_plain(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.len() < 40 || t.lines().count() <= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize_html ───────────────────────────────────────────

    #[test]
    fn strips_script_and_style() {
        let html = r#"
            <style>.foo { color: red; }</style>
            <script>alert('xss')</script>
            <p>Safe content</p>
        "#;
        let result = sanitize_html(html);
        assert!(result.contains("Safe content"));
        assert!(!result.contains("color: red"));
        assert!(!result.contains("alert"));
    }

    #[test]
    fn strips_tracking_pixels() {
        let html = r#"<p>Real content</p><img src="https://track.example.com/open.gif" width="1" height="1">"#;
        let result = sanitize_html(html);
        assert!(result.contains("Real content"));
        assert!(!result.contains("track.example.com"));
    }

    #[test]
    fn strips_layout_tables() {
        let html = r#"
            <table><tr><td>
                <p>Actual message</p>
            </td></tr></table>
        "#;
        let result = sanitize_html(html);
        assert!(result.contains("Actual message"));
        assert!(!result.contains("|"));
    }

    #[test]
    fn preserves_links() {
        let html = r#"<p>Click <a href="https://example.com">here</a></p>"#;
        let result = sanitize_html(html);
        assert!(result.contains("https://example.com"));
        assert!(result.contains("here"));
    }

    #[test]
    fn preserves_formatting() {
        let html = "<p>This is <strong>bold</strong> and <em>italic</em></p>";
        let result = sanitize_html(html);
        assert!(result.contains("**bold**") || result.contains("__bold__"));
        assert!(result.contains("*italic*") || result.contains("_italic_"));
    }

    #[test]
    fn preserves_lists() {
        let html = "<ul><li>one</li><li>two</li><li>three</li></ul>";
        let result = sanitize_html(html);
        assert!(result.contains("one"));
        assert!(result.contains("two"));
        assert!(result.contains("three"));
    }

    #[test]
    fn preserves_blockquotes() {
        let html = "<blockquote><p>Quoted text</p></blockquote>";
        let result = sanitize_html(html);
        assert!(result.contains("Quoted text"));
    }

    #[test]
    fn strips_forms() {
        let html = r#"<form action="/phish"><input type="text"><button>Submit</button></form><p>Content</p>"#;
        let result = sanitize_html(html);
        assert!(result.contains("Content"));
        assert!(!result.contains("<form"));
        assert!(!result.contains("<input"));
    }

    #[test]
    fn strips_iframes() {
        let html = r#"<iframe src="https://evil.com"></iframe><p>Content</p>"#;
        let result = sanitize_html(html);
        assert!(result.contains("Content"));
        assert!(!result.contains("evil.com"));
    }

    #[test]
    fn truncates_huge_input() {
        let huge = "<p>".to_string() + &"x".repeat(DEFAULT_MAX_HTML_BYTES + 1000) + "</p>";
        let result = sanitize_html(&huge);
        assert!(result.len() <= DEFAULT_MAX_MD_CHARS);
    }

    #[test]
    fn empty_html_produces_empty_output() {
        let result = sanitize_html("");
        assert!(result.trim().is_empty());
    }

    // ── sanitize_html_with (custom config) ──────────────────────

    #[test]
    fn custom_config_extra_tags() {
        let mut config = Config::default();
        config.extra_tags.insert("img".to_string());
        let html = r#"<p>Text</p><img alt="photo">"#;
        let result = sanitize_html_with(html, &config);
        assert!(result.contains("Text"));
    }

    #[test]
    fn custom_config_smaller_limits() {
        let config = Config {
            max_html_bytes: 100,
            max_md_chars: 50,
            extra_tags: HashSet::new(),
        };
        let html = "<p>".to_string() + &"word ".repeat(200) + "</p>";
        let result = sanitize_html_with(&html, &config);
        assert!(result.len() <= 50);
    }

    // ── render_email ────────────────────────────────────────────

    #[test]
    fn prefers_real_plain_text() {
        let plain = "Hey,\n\nThis is a real email body with enough content to pass the junk filter.\n\nCheers";
        let html = "<p>HTML version</p>";
        let result = render_email(Some(plain), Some(html));
        assert_eq!(result, plain);
    }

    #[test]
    fn skips_junk_plain_for_html() {
        let junk = "View online";
        let html = "<p>This is the <strong>real</strong> email content right here.</p>";
        let result = render_email(Some(junk), Some(html));
        assert_ne!(result, junk);
        assert!(result.contains("real"));
    }

    #[test]
    fn shows_junk_plain_when_no_html() {
        let junk = "View online";
        let result = render_email(Some(junk), None);
        assert_eq!(result, junk);
    }

    #[test]
    fn no_content_fallback() {
        assert_eq!(render_email(None, None), "[No displayable content]");
    }

    #[test]
    fn html_only_sanitizes() {
        let html = r#"<style>evil</style><p>Good <strong>content</strong></p><img src="tracker.gif">"#;
        let result = render_email(None, Some(html));
        assert!(result.contains("Good"));
        assert!(result.contains("**content**") || result.contains("__content__"));
        assert!(!result.contains("evil"));
        assert!(!result.contains("tracker"));
    }

    // ── render_email_plain ──────────────────────────────────────

    #[test]
    fn plain_text_output_prefers_plain() {
        let result = render_email_plain(Some("Hello, world"), Some("<p>Hello, world</p>"));
        assert_eq!(result, "Hello, world");
    }

    #[test]
    fn plain_text_falls_back_to_html() {
        let result = render_email_plain(None, Some("<p>Hello</p>"));
        assert!(!result.is_empty());
        assert!(result.contains("Hello"));
        assert!(!result.contains("<p>"));
    }

    #[test]
    fn plain_text_no_content() {
        assert_eq!(render_email_plain(None, None), "[No displayable content]");
    }

    // ── is_junk_plain ───────────────────────────────────────────

    #[test]
    fn empty_is_junk() {
        assert!(is_junk_plain(""));
    }

    #[test]
    fn whitespace_only_is_junk() {
        assert!(is_junk_plain("   \n\t  \n  "));
    }

    #[test]
    fn short_stub_is_junk() {
        assert!(is_junk_plain("Click here to view"));
    }

    #[test]
    fn real_content_not_junk() {
        let real = "Hey,\n\nJust wanted to follow up on our conversation from yesterday.\n\nLet me know what you think.\n\nThanks";
        assert!(!is_junk_plain(real));
    }
}
