//! SynergyCoder's free OpenRouter route, shared by Assistant and Ask.
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::openai::{build_body, run_openai};
use super::{ChatProvider, ProviderConfig};
use crate::app::chat::types::{ChatEvent, Message, ToolDef};

const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

pub struct SycFree;

fn free_body(
    cfg: &ProviderConfig,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
) -> Result<Value, String> {
    // Restrict names as well as prices: a custom suffix such as :online
    // enables a separately billed server-side tool.
    let free_variant = cfg
        .model
        .strip_suffix(":free")
        .and_then(|id| id.split_once('/'))
        .is_some_and(|(owner, model)| {
            [owner, model].iter().all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            })
        });
    if cfg.model != "openrouter/free" && !free_variant {
        return Err("SYC Free requires openrouter/free or a provider/model:free ID".into());
    }
    let mut body = build_body(cfg, system, messages, tools, "max_tokens");
    // A free reasoning model can use the entire output budget on hidden
    // reasoning. Ask for an immediate answer unless the profile requests it.
    if cfg.reasoning.is_none() {
        body["reasoning"] = json!({"effort": "none"});
    }
    body["provider"] =
        json!({"max_price": {"prompt": 0, "completion": 0, "request": 0, "image": 0}});
    Ok(body)
}

impl ChatProvider for SycFree {
    fn name(&self) -> &'static str {
        "syc_free"
    }

    fn stream_turn(
        &self,
        cfg: &ProviderConfig,
        system: &str,
        messages: &[Message],
        tools: &[ToolDef],
        cancel: &AtomicBool,
        sink: &mut dyn FnMut(ChatEvent),
    ) -> Result<(), String> {
        let body = free_body(cfg, system, messages, tools)?;
        let headers = [("authorization", format!("Bearer {}", cfg.api_key))];
        run_openai(ENDPOINT, &headers, &body, cancel, sink)
    }
}

#[cfg(test)]
#[path = "syc_free_tests.rs"]
mod tests;
