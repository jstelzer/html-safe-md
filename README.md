# html-safe-md

**Status:** Design / Pre-extraction
**Origin:** Extracted from [nevermail](https://github.com/jstelzer/nevermail) `src/core/mime.rs`
**License:** Apache-2.0 / MIT (dual-licensed — no melib dependency, no GPL obligation)

---

## What This Is

A Rust crate for converting untrusted HTML into safe, readable markdown. Built for email but useful anywhere you need to render HTML from an untrusted source without a webview, remote fetches, or JavaScript execution.

The pitch: **display rich content without surrendering privacy or security.**

## The Problem

HTML email is a privacy and security minefield:
- Tracking pixels (`<img src="https://track.example.com/open.gif" width="1" height="1">`)
- Remote image loads that leak IP, client, and read-time to senders
- JavaScript (rare in email, devastating when it lands)
- CSS that exfiltrates data or fingerprints renderers
- MSO conditionals and layout tables that produce garbage when naively converted

The standard approaches are both bad:
1. **Full webview rendering** (Thunderbird, most GUI clients) — executes the payload. You're running the sender's code.
2. **Plain text only** (mutt, stripping everything) — loses all structure, links, formatting. Newsletters become walls of text.

## The Solution

A middle path: **sanitize HTML down to semantic content, then convert to markdown.**

```
untrusted HTML
  → ammonia (allowlist-based sanitizer, strips everything dangerous)
  → html2md (structural conversion to markdown)
  → safe markdown string
```

The output is a plain markdown string. What you do with it — render in iced, ratatui, a terminal pager, write to disk — is your business. The crate handles the dangerous part.

## Pipeline Detail

### Stage 1: Sanitize (ammonia)

ammonia is an allowlist-based HTML sanitizer. The default allowlist is tuned for "safe for browsers" which still permits layout tables, inline styles, and structural junk that produces garbage markdown. We override with an email-specific allowlist:

**Allowed tags:**
- Block: `p`, `br`, `hr`, `blockquote`, `pre`
- Headings: `h1`–`h6`
- Inline: `b`, `strong`, `i`, `em`, `code`, `s`, `del`, `u`, `small`, `sub`, `sup`
- Lists: `ul`, `ol`, `li`
- Links: `a` (ammonia sanitizes `href` — no `javascript:` URIs)

**Everything else is stripped.** Text content inside stripped tags is preserved — only the tags themselves are removed. This means a `<table><tr><td>Real content</td></tr></table>` becomes just "Real content" rather than a mangled markdown table.

**What gets killed:**
- `<img>` — no remote fetches, no tracking pixels, no beacon GIFs
- `<table>`, `<tr>`, `<td>` — layout tables produce markdown table soup
- `<style>` — CSS exfiltration, fingerprinting
- `<script>` — obvious
- `<iframe>`, `<object>`, `<embed>` — embedded content
- `<form>`, `<input>` — phishing vectors
- MSO conditionals (`<!--[if mso]>`) — Office junk
- All inline `style=""` attributes — stripped by ammonia when not in the allowed set

### Stage 2: Convert (html2md)

The sanitized HTML is now clean semantic markup. html2md converts it to markdown:
- `<strong>` → `**bold**`
- `<em>` → `*italic*`
- `<a href="...">text</a>` → `[text](...)`
- `<blockquote>` → `> quoted`
- `<ul><li>` → `- item`
- `<h1>` → `# heading`

The output is standard CommonMark-compatible markdown.

### Stage 3: Safety Limits

- **Input truncation:** HTML larger than 512 KB is truncated before processing. Marketing emails occasionally embed enormous base64 blobs or repeated template blocks.
- **Output cap:** Markdown output truncated to 200K chars. Belt and suspenders.

## Junk Detection

Many emails include both `text/plain` and `text/html` parts. The plain version is usually preferable (already safe, no conversion needed). But some senders use the plain part as a stub:

```
View this email in your browser
```

The crate includes a junk detector for plain text parts:
- Empty or whitespace-only → junk
- Under 40 characters → junk
- Two or fewer lines → junk

When plain text is junk, the pipeline falls through to the HTML sanitization path. When there's no HTML either, junk plain text is shown as-is (something is better than nothing).

## Planned API Surface

```rust
/// Configuration for the sanitizer.
pub struct SanitizerConfig {
    /// Max HTML input size in bytes before truncation.
    pub max_html_bytes: usize,        // default: 512 * 1024
    /// Max markdown output length in chars.
    pub max_md_chars: usize,          // default: 200_000
    /// Additional tags to allow beyond the default email set.
    pub extra_tags: HashSet<String>,
    /// Additional tags to deny (applied after allow, for overrides).
    pub deny_tags: HashSet<String>,
}

/// Sanitize raw HTML and convert to markdown.
pub fn sanitize_html(html: &str, config: &SanitizerConfig) -> String;

/// Sanitize HTML using default email-tuned config.
pub fn sanitize_html_default(html: &str) -> String;

/// Render email body to markdown, preferring plain text when available.
/// Falls back to HTML sanitization when plain text is missing or junk.
pub fn render_email(text_plain: Option<&str>, text_html: Option<&str>, config: &SanitizerConfig) -> String;

/// Returns true if the plain-text part looks like a stub or tracking junk.
pub fn is_junk_plain(text: &str) -> bool;
```

The `sanitize_html` function is the core — usable outside email contexts (app store descriptions, RSS feeds, CMS content, anywhere untrusted HTML appears).

The `render_email` function is the convenience wrapper that handles the plain-vs-HTML decision and junk detection.

## Dependencies

- `ammonia` — HTML sanitization (well-maintained, widely used, no unsafe)
- `html2md` — HTML to markdown conversion
- `html2text` — HTML to plain text fallback (optional feature, for consumers that want text output instead of markdown)

No runtime dependencies on any UI framework. The crate produces strings. Rendering is the consumer's problem.

## Potential Consumers

1. **nevermail** — the origin. `core/mime.rs` would become a thin wrapper around this crate.
2. **cosmic-store** — COSMIC's app store needs to safely render app descriptions from upstream. Same trust boundary, same problem.
3. **Any email client** — the pipeline is framework-agnostic.
4. **RSS/feed readers** — untrusted HTML from feeds, same threat model.
5. **Documentation renderers** — anywhere user-submitted HTML needs safe display.

## Design Principles

- **Allowlist, not denylist.** Only explicitly permitted tags survive. New HTML features are blocked by default.
- **Text content is sacred.** Stripped tags lose their markup, not their content. You never lose the message.
- **No remote fetches.** Zero network activity during sanitization. If it's not inline, it doesn't exist.
- **No framework coupling.** Output is a string. Use it with iced, ratatui, egui, or `println!`.
- **Sane defaults, configurable when needed.** The default config is tuned for email. Override for other contexts.

## Open Questions

- **License:** [Decided] Apache-2.0 / MIT dual license. The crate has zero melib dependency — ammonia, html2md, and html2text are all MIT-compatible. No GPL obligation. This maximizes adoption (cosmic-store, other projects can pull it in without license friction).
- **Image alt text:** Currently `<img>` is stripped entirely. Should we preserve `alt` attributes as `[image: alt text]` in the output? Useful for accessibility.
- **Link rewriting:** Should we offer an option to defang tracking URLs (strip UTM params, unwrap redirect wrappers)?
- **Crate name:** [Decided] `html-safe-md`. Discoverable, describes what it does, no brand gatekeeping. Someone searching crates.io for the problem finds the solution.

---

*"Display the message. Not the sender's agenda."*
