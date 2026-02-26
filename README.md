# html-safe-md

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

Convert untrusted HTML into safe, readable markdown. No remote fetches, no scripts, no tracking pixels.

Built for email. Useful anywhere you need to render HTML from an untrusted source without a webview or JavaScript execution.

## Quick Start

```rust
// Sanitize HTML to markdown with default email-tuned config
let md = html_safe_md::sanitize_html("<p>Hello <strong>world</strong></p>");
assert!(md.contains("**world**"));

// Render an email body (prefers plain text, falls back to sanitized HTML)
let md = html_safe_md::render_email(
    Some("Hey, just following up on our conversation from yesterday.\n\nLet me know."),
    Some("<p>HTML version</p>"),
);
assert!(md.contains("following up"));
```

## The Problem

HTML in email (and app stores, RSS feeds, CMS content) is a privacy and security minefield:

- **Tracking pixels** — `<img>` tags that phone home when you open a message
- **Remote image loads** — leak your IP, client, and read-time to senders
- **JavaScript** — rare in email, devastating when it lands
- **CSS exfiltration** — fingerprinting and data leaks via style rules
- **Layout tables** — produce garbage when naively converted to markdown

The standard approaches are both bad:

1. **Full webview rendering** — you're running the sender's code
2. **Plain text only** — loses all structure, links, formatting

## The Solution

A middle path: **sanitize HTML down to semantic content, then convert to markdown.**

```text
untrusted HTML
  → ammonia (allowlist-based sanitizer, strips everything dangerous)
  → html2md (structural conversion to markdown)
  → safe markdown string
```

The output is a plain string. What you render it with — iced, ratatui, egui, a terminal pager — is your business.

## What Survives

Only semantic tags that produce meaningful markdown:

- **Block:** `p`, `br`, `hr`, `blockquote`, `pre`
- **Headings:** `h1`–`h6`
- **Inline:** `b`, `strong`, `i`, `em`, `code`, `s`, `del`, `u`, `small`, `sub`, `sup`
- **Lists:** `ul`, `ol`, `li`
- **Links:** `a` (ammonia sanitizes `href` — no `javascript:` URIs)

## What Gets Killed

Everything else. Text content inside stripped tags is preserved — only the tags are removed.

- `<img>` — no remote fetches, no tracking pixels, no beacon GIFs
- `<table>`, `<tr>`, `<td>` — layout tables that produce markdown soup
- `<style>` — CSS exfiltration and fingerprinting
- `<script>` — obvious
- `<iframe>`, `<object>`, `<embed>` — embedded content
- `<form>`, `<input>` — phishing vectors
- MSO conditionals — Office junk
- All inline `style=""` attributes

## API

```rust
// Core: sanitize any untrusted HTML to markdown
html_safe_md::sanitize_html(html: &str) -> String

// With custom config (extra tags, size limits)
html_safe_md::sanitize_html_with(html: &str, config: &Config) -> String

// Email convenience: prefers plain text, falls back to sanitized HTML
html_safe_md::render_email(text_plain: Option<&str>, text_html: Option<&str>) -> String

// Email plain text output (no markdown, uses html2text for fallback)
html_safe_md::render_email_plain(text_plain: Option<&str>, text_html: Option<&str>) -> String

// Detect junk/stub plain text parts
html_safe_md::is_junk_plain(text: &str) -> bool
```

### Configuration

```rust
use html_safe_md::Config;

let config = Config {
    max_html_bytes: 256 * 1024,  // default: 512 KB
    max_md_chars: 100_000,       // default: 200K
    extra_tags: Default::default(),
};
let md = html_safe_md::sanitize_html_with(html, &config);
```

## Safety Limits

- **Input truncation:** HTML larger than 512 KB is truncated before processing
- **Output cap:** Markdown output capped at 200K characters

## Email Junk Detection

Many emails include both `text/plain` and `text/html`. The plain version is preferred (already safe), but some senders use it as a stub:

```
View this email in your browser
```

`render_email` detects these stubs (empty, under 40 chars, or ≤2 lines) and falls through to the HTML sanitization pipeline.

## Design Principles

- **Allowlist, not denylist.** New HTML features are blocked by default.
- **Text content is sacred.** Stripped tags lose their markup, not their content.
- **No remote fetches.** Zero network activity. If it's not inline, it doesn't exist.
- **No framework coupling.** Output is a string. Rendering is your problem.
- **Sane defaults, configurable when needed.** Tuned for email. Override for other contexts.

## Use Cases

- **Email clients** — [nevermail](https://github.com/jstelzer/nevermail) (the origin of this crate)
- **App stores** — safely render app descriptions from upstream sources
- **RSS/feed readers** — untrusted HTML from feeds, same threat model
- **CMS/comment systems** — anywhere user-submitted HTML needs safe display

## Dependencies

- [`ammonia`](https://crates.io/crates/ammonia) — HTML sanitization
- [`html2md`](https://crates.io/crates/html2md) — HTML to markdown conversion
- [`html2text`](https://crates.io/crates/html2text) — HTML to plain text fallback

No UI framework dependencies. Stable Rust. No unsafe.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

---

*"Display the message. Not the sender's agenda."*
