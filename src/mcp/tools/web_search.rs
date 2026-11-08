//! MCP tool: `web_search` - query a web search engine and return the top
//! results as `{title, url, snippet}` triples.
//!
//! This is the discovery half of "compare a row against the live web":
//! a lot title goes in, candidate listing pages come out, and `web_fetch`
//! reads each one. Bing is the backend because its result page is plain
//! HTML that still renders server-side - no API key, no JavaScript, no
//! bot wall on a browser-shaped request.

use base64::Engine;
use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::mcp::OctaMcpServer;

use super::ToolContext;
use super::web_fetch;

pub const DESCRIPTION: &str = "Search the web and return the top results as {title, url, snippet} \
triples. `query` is the search text, `max_results` caps the list (default 5, hard 10). Use it to \
find a product listing, a price comparison or any page a row refers to - then read a hit with \
web_fetch.";

/// Enough results to pick a match without flooding the turn.
const DEFAULT_MAX_RESULTS: usize = 5;
const HARD_MAX_RESULTS: usize = 10;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// The search text - a product name, a lot title, any query.
    pub query: String,
    /// Maximum results returned. Default 5, hard cap 10.
    #[serde(default)]
    pub max_results: Option<usize>,
}

/// One parsed result line.
#[derive(Debug)]
pub(crate) struct Hit {
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) snippet: String,
}

/// Result blocks in a Bing SERP are `<li class="b_algo">` elements; the title
/// link lives in the block's `<h2><a ... href="...">`.
fn parse_results(html: &str, max: usize) -> Vec<Hit> {
    let block_re = regex::Regex::new(r#"(?s)<li class="b_algo"[^>]*>(.*?)</li>"#).unwrap();
    let link_re =
        regex::Regex::new(r#"(?s)<h2[^>]*><a[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#).unwrap();
    let snip_re = regex::Regex::new(r#"(?s)<p[^>]*>(.*?)</p>"#).unwrap();
    let tag_re = regex::Regex::new(r#"<[^>]+>"#).unwrap();

    block_re
        .captures_iter(html)
        .filter_map(|block| {
            let inner = block.get(1)?.as_str();
            let link = link_re.captures(inner)?;
            let raw_url = web_fetch::decode_entities(&link[1]);
            let url = unwrap_bing_redirect(&raw_url);
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return None;
            }
            let title = collapse(&tag_re.replace_all(&link[2], ""));
            let snippet = snip_re
                .captures(inner)
                .map(|s| collapse(&tag_re.replace_all(&s[1], "")))
                .unwrap_or_default();
            Some(Hit {
                title,
                url,
                snippet,
            })
        })
        .take(max)
        .collect()
}

/// Bing wraps outbound links in `bing.com/ck/a?...&u=a1<base64url>&...`
/// redirect hops - the `u` value is the real URL base64url-encoded behind a
/// two-char marker. Decode it so callers fetch the page, not the tracker.
fn unwrap_bing_redirect(url: &str) -> String {
    let Some(pos) = url.find("bing.com/ck/a?") else {
        return url.to_string();
    };
    let Some(u) = url[pos..]
        .split('&')
        .find_map(|kv| kv.strip_prefix("u="))
        .and_then(|v| v.split('&').next())
    else {
        return url.to_string();
    };
    // Strip the "a1" version marker Bing prepends to the base64 payload.
    let Some(payload) = u.get(2..) else {
        return url.to_string();
    };
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|s| s.starts_with("http"))
        .unwrap_or_else(|| url.to_string())
}

/// Entities decoded, whitespace flattened - one line per field.
fn collapse(text: &str) -> String {
    web_fetch::decode_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Search and return the parsed hits - the tool wrapper and `enrich_rows`
/// (query column -> top hit -> fetch) share this.
pub(crate) fn search(query: &str, max_results: usize) -> anyhow::Result<Vec<Hit>> {
    let max = max_results.clamp(1, HARD_MAX_RESULTS);
    let url = format!(
        "https://www.bing.com/search?q={}&count={}",
        urlencoded(query),
        max + 4 // extra headroom: some blocks are ads/news, not results
    );
    // SERPs go through the shared page cache: a batch whose rows repeat a
    // title queries Bing once per unique query per TTL, not once per row.
    let (_, status, _, body, _) = web_fetch::fetch_cached_stamped(&url, false)?;
    if status != 200 {
        anyhow::bail!("search returned HTTP {status}");
    }
    Ok(parse_results(&body, max))
}

pub fn run(_ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let query = p.query.trim();
    if query.is_empty() {
        anyhow::bail!("query must not be empty");
    }
    let hits = search(query, p.max_results.unwrap_or(DEFAULT_MAX_RESULTS))?;

    let mut out = Map::new();
    out.insert("query".to_string(), Value::String(query.to_string()));
    out.insert("count".to_string(), Value::from(hits.len()));
    out.insert(
        "results".to_string(),
        Value::Array(
            hits.into_iter()
                .map(|h| {
                    serde_json::json!({
                        "title": h.title,
                        "url": h.url,
                        "snippet": h.snippet,
                    })
                })
                .collect(),
        ),
    );
    Ok(Value::Object(out))
}

/// Minimal percent-encoding for a query string value - UTF-8 safe, keeps
/// unreserved characters verbatim.
fn urlencoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for &b in text.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("web_search failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
#[path = "web_search_tests.rs"]
mod tests;
