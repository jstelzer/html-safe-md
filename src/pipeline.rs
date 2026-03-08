//! Internal pipeline stages: pre-processing, sanitization, post-processing.
//!
//! These are pure functions that transform HTML or markdown strings. None of
//! them are public — they're called by the top-level `sanitize_html_with`,
//! `render_email_with`, and `render_email_plain` functions in `lib.rs`.

use std::collections::HashSet;

use crate::{Config, ALLOWED_TAGS, MAX_URL_LEN};

/// Inject `<br>` after block-level closing tags that ammonia will strip.
///
/// When ammonia removes `<td>`, `<div>`, `<th>`, etc., text from adjacent
/// cells/blocks collapses into one line. Inserting a break before stripping
/// preserves the visual separation the original layout intended.
pub(crate) fn prep_block_breaks(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 1024);
    let mut rest = html;
    while let Some(pos) = rest.find("</") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        if let Some(end) = after.find('>') {
            let tag = after[..end].trim().to_ascii_lowercase();
            let is_block = matches!(
                tag.as_str(),
                "td" | "th" | "div" | "tr" | "table" | "section" | "article" | "footer"
                    | "header" | "nav" | "aside" | "main" | "center"
            );
            out.push_str(&rest[pos..pos + 2 + end + 1]);
            if is_block {
                out.push_str("<br>");
            }
            rest = &after[end + 1..];
        } else {
            // Malformed — push the rest and bail
            out.push_str(&rest[pos..]);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

/// Strip HTML down to semantic content using an allowlist.
pub(crate) fn clean_html(html: &str, config: &Config) -> String {
    let mut tags: HashSet<&str> = ALLOWED_TAGS.iter().copied().collect();
    let extras: Vec<&str> = config.extra_tags.iter().map(|s| s.as_str()).collect();
    tags.extend(extras);
    ammonia::Builder::new().tags(tags).clean(html).to_string()
}

/// Post-process markdown output from html2md.
///
/// - Drops markdown links where the URL exceeds MAX_URL_LEN (keeps link text)
/// - Removes unnecessary backslash-escapes on underscores
/// - Collapses 3+ consecutive blank lines to 2
pub(crate) fn post_process_md(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut chars = md.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '[' {
            // Potential markdown link — collect [text](url)
            let mut text = String::new();
            let mut found_link = false;
            let mut depth = 1;
            for c in chars.by_ref() {
                if c == '[' {
                    depth += 1;
                    text.push(c);
                } else if c == ']' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    text.push(c);
                } else {
                    text.push(c);
                }
            }
            if chars.peek() == Some(&'(') {
                chars.next();
                let mut url = String::new();
                let mut paren_depth = 1;
                for c in chars.by_ref() {
                    if c == '(' {
                        paren_depth += 1;
                        url.push(c);
                    } else if c == ')' {
                        paren_depth -= 1;
                        if paren_depth == 0 {
                            break;
                        }
                        url.push(c);
                    } else {
                        url.push(c);
                    }
                }
                if url.len() > MAX_URL_LEN {
                    out.push_str(&text);
                } else {
                    out.push('[');
                    out.push_str(&text);
                    out.push_str("](");
                    out.push_str(&url);
                    out.push(')');
                }
                found_link = true;
            }
            if !found_link {
                out.push('[');
                out.push_str(&text);
                out.push(']');
            }
        } else if ch == '\\' {
            // Remove unnecessary backslash-escapes on underscores
            match chars.peek() {
                Some('_') => {
                    out.push('_');
                    chars.next();
                }
                _ => out.push(ch),
            }
        } else {
            out.push(ch);
        }
    }

    collapse_blank_lines(&out)
}

/// Post-process plain text output from html2text.
///
/// - Strips lines that are only box-drawing / decoration characters
/// - Strips reference-style link definitions where the full URL exceeds MAX_URL_LEN
/// - Collapses 3+ consecutive blank lines to 2
pub(crate) fn post_process_plain(text: &str) -> String {
    let all_lines: Vec<&str> = text.lines().collect();
    let mut keep = vec![true; all_lines.len()];

    // First pass: identify and mark long reference URLs for removal.
    // html2text wraps long URLs across multiple continuation lines.
    let mut i = 0;
    while i < all_lines.len() {
        let trimmed = all_lines[i].trim();
        if trimmed.starts_with('[') {
            if let Some(colon_pos) = trimmed.find("]: ") {
                let ref_id = &trimmed[1..colon_pos];
                if ref_id.chars().all(|c| c.is_ascii_digit()) {
                    let url_start = &trimmed[colon_pos + 3..];
                    let mut url_len = url_start.len();
                    let first_line = i;
                    let mut j = i + 1;
                    while j < all_lines.len() {
                        let next = all_lines[j].trim();
                        if !next.is_empty() && !next.starts_with('[') {
                            url_len += next.len();
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    if url_len > MAX_URL_LEN {
                        for item in keep.iter_mut().take(j).skip(first_line) {
                            *item = false;
                        }
                    }
                    i = j;
                    continue;
                }
            }
        }
        i += 1;
    }

    // Second pass: filter decoration and collapse blank lines
    let mut result_lines: Vec<&str> = Vec::new();
    let mut blank_count = 0;

    for (idx, line) in all_lines.iter().enumerate() {
        if !keep[idx] {
            continue;
        }
        let trimmed = line.trim();

        // Skip lines that are pure decoration (box-drawing, dashes, underscores)
        if !trimmed.is_empty() && is_decoration(trimmed) {
            continue;
        }

        if trimmed.is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result_lines.push(line);
            }
        } else {
            blank_count = 0;
            result_lines.push(line);
        }
    }

    result_lines.join("\n").trim().to_string()
}

/// Collapse 3+ consecutive newlines to 2 and trim.
fn collapse_blank_lines(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut newline_count = 0;
    for ch in s.chars() {
        if ch == '\n' {
            newline_count += 1;
            if newline_count <= 2 {
                result.push(ch);
            }
        } else {
            newline_count = 0;
            result.push(ch);
        }
    }
    result.trim().to_string()
}

/// True if every character is a box-drawing or decoration character.
fn is_decoration(s: &str) -> bool {
    s.chars()
        .all(|c| "─━┄┈═╌╍┅┉│┃┆┊║╎╏┇┋├┤┬┴┼╔╗╚╝╠╣╦╩╬┌┐└┘-_=|+".contains(c))
}
