//! MCP tool: `web_fetch` - fetch an http(s) URL and return its text.
//!
//! HTML pages come back as readable plain text (scripts, styles and markup
//! dropped); `raw: true` returns the body verbatim instead. This is the
//! read half of "compare a row against the live web": the lot page or the
//! source a cell links to is one call away, no browser in the loop.

use std::time::Duration;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};
use ureq::ResponseExt;

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Fetch an http(s) URL and return its content as text. HTML pages are converted \
to readable plain text (scripts, styles and tags dropped); pass `raw: true` to keep the body \
verbatim. `max_chars` caps the returned text (default 60000). Returns `{url, final_url, status, \
content_type, char_count, truncated, text}`. Use it to read a product page, a listing, or an HTTP \
endpoint a cell links to.";

/// One request's hard ceiling: fetching is a read, never a download.
const TIMEOUT_SECS: u64 = 20;
/// The most body bytes accepted from one response.
const MAX_BODY_BYTES: u64 = 4 * 1024 * 1024;
/// Default text cap - a page worth of content without flooding the turn.
const DEFAULT_MAX_CHARS: usize = 60_000;
/// Nobody needs more than this in one tool result.
const HARD_MAX_CHARS: usize = 500_000;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// The http:// or https:// address to fetch.
    pub url: String,
    /// Maximum characters of text to return. Default 60000, hard cap 500000.
    #[serde(default)]
    pub max_chars: Option<usize>,
    /// Return the body verbatim instead of converting HTML to text.
    #[serde(default)]
    pub raw: bool,
}

/// Fetch `url`, following redirects, and return `(final_url, status,
/// content_type, body_text)`. The body is capped at [`MAX_BODY_BYTES`] and
/// decoded as lossy UTF-8 - this tool is for reading pages, not for
/// downloading binaries.
pub(crate) fn fetch(url: &str) -> anyhow::Result<(String, u16, String, String)> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(TIMEOUT_SECS)))
        // Product pages bounce through short links and locale redirects;
        // landing where they point is the point of the fetch. Five hops
        // still breaks a loop instead of hanging on it.
        .max_redirects(5)
        .user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
             AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36",
        )
        .build()
        .new_agent();
    let mut resp = agent
        .get(url)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/json,text/plain;q=0.9,*/*;q=0.5",
        )
        .call()
        .map_err(|e| anyhow::anyhow!("fetching {url}: {e:#}"))?;
    let status = resp.status().as_u16();
    let final_url = resp.get_uri().to_string();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = resp
        .body_mut()
        .with_config()
        .limit(MAX_BODY_BYTES)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(|e| anyhow::anyhow!("reading {url}: {e:#}"))?;
    Ok((final_url, status, content_type, body))
}

/// Tags whose content is dropped entirely - none of it is page text.
const DROP_CONTENT: &[&str] = &["script", "style", "noscript", "svg", "template"];

/// Tags that break a line when they open or close - the text a person
/// would see is roughly the text between them.
const BLOCK_TAGS: &[&str] = &[
    "p",
    "div",
    "br",
    "hr",
    "li",
    "ul",
    "ol",
    "tr",
    "td",
    "th",
    "table",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "main",
    "aside",
    "figure",
    "figcaption",
    "blockquote",
    "pre",
    "form",
    "fieldset",
    "dl",
    "dt",
    "dd",
];

/// The tag name inside `<...>`: lowercased, attributes and closing slash
/// stripped. Returns `None` for comments and declarations.
fn tag_name(inner: &str) -> Option<&str> {
    let inner = inner.trim();
    if inner.starts_with('!') || inner.starts_with('?') {
        return None;
    }
    let name = inner
        .trim_start_matches('/')
        .split(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
        .next()?;
    if name.is_empty() {
        return None;
    }
    Some(name)
}

/// Decode the common named and numeric entities. Anything else survives
/// verbatim - mangling a stray `&` is worse than leaving it.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos + 1..];
        let Some(semi) = tail.find(';') else {
            out.push('&');
            rest = tail;
            continue;
        };
        let (entity, advance) = (&tail[..semi], semi + 1);
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => {
                entity[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        if let Some(c) = decoded {
            out.push(c);
            rest = &tail[advance..];
        } else {
            out.push('&');
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

/// HTML to readable text: drop script/style/svg content, turn block-level
/// markup into line breaks, strip the rest, decode entities, collapse
/// whitespace. Not a layout engine - the model gets the words, in order.
pub(crate) fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut drop_depth: usize = 0;
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        let chunk = &rest[..open];
        if drop_depth == 0 {
            out.push_str(&decode_entities(chunk));
        }
        let tail = &rest[open + 1..];
        let Some(close) = tail.find('>') else {
            // An unterminated tag ends the page; the text before it was
            // already flushed, and nothing readable comes after a lone '<'.
            rest = "";
            break;
        };
        let inner = &tail[..close];
        rest = &tail[close + 1..];
        let Some(name) = tag_name(inner) else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        let is_close = inner.trim_start().starts_with('/');
        if DROP_CONTENT.contains(&name.as_str()) {
            if is_close {
                drop_depth = drop_depth.saturating_sub(1);
            } else if !inner.trim_end().ends_with('/') {
                drop_depth += 1;
            }
            continue;
        }
        if drop_depth > 0 {
            continue;
        }
        if BLOCK_TAGS.contains(&name.as_str()) {
            out.push('\n');
        }
    }
    // Text after the last tag still counts.
    if drop_depth == 0 {
        out.push_str(&decode_entities(rest));
    }
    // Collapse every whitespace run to one space, one blank line max.
    let mut compact = String::with_capacity(out.len());
    let mut blank_seen = false;
    for line in out.lines() {
        let line: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            if !compact.is_empty() && !blank_seen {
                compact.push('\n');
                blank_seen = true;
            }
            continue;
        }
        blank_seen = false;
        compact.push_str(&line);
        compact.push('\n');
    }
    compact.trim().to_string()
}

/// True when a content-type header names an HTML payload.
fn is_html(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .eq_ignore_ascii_case("text/html")
}

pub fn run(_ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let url = p.url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        anyhow::bail!("url must start with http:// or https://: {url}");
    }
    let max_chars = p
        .max_chars
        .unwrap_or(DEFAULT_MAX_CHARS)
        .clamp(1, HARD_MAX_CHARS);
    let (final_url, status, content_type, body) = fetch(url)?;
    let is_html_body =
        is_html(&content_type) || (content_type.is_empty() && body.trim_start().starts_with('<'));
    let text = if p.raw || !is_html_body {
        body
    } else {
        html_to_text(&body)
    };
    let char_count = text.chars().count();
    let truncated = char_count > max_chars;
    let text: String = text.chars().take(max_chars).collect();

    let mut out = Map::new();
    out.insert("url".to_string(), Value::String(url.to_string()));
    out.insert("final_url".to_string(), Value::String(final_url));
    out.insert("status".to_string(), Value::from(status));
    out.insert("content_type".to_string(), Value::String(content_type));
    out.insert("char_count".to_string(), Value::from(char_count));
    out.insert("truncated".to_string(), Value::Bool(truncated));
    out.insert("text".to_string(), Value::String(text));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("web_fetch failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
#[path = "web_fetch_tests.rs"]
mod tests;
