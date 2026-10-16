//! Reading the text out of a model's reply, for the built-in AI features.
//!
//! Gateways differ a lot: content as a string or as a list of parts, thinking
//! blocks before the text, replies streamed as server-sent events even when no
//! stream was asked for, or the whole budget spent on reasoning so no text
//! comes back. This turns all of those into either the text or a message that
//! says what happened.

use serde_json::Value;

/// Why no text could be read.
#[derive(Debug, PartialEq)]
pub enum NoText {
    /// The reply stopped at the token limit before any text (usually a
    /// reasoning model thinking for the whole budget).
    Truncated,
    /// The reply was cut off at the token limit part-way through the text;
    /// carries what did arrive (the app can still use the complete part).
    Partial(String),
    /// The model refused or a filter blocked the answer.
    Refused(String),
    /// Something else; carries a short excerpt of the reply.
    Unknown(String),
}

impl NoText {
    pub fn message(&self) -> String {
        match self {
            NoText::Partial(_) => "模型的回复写到一半被截断了".into(),
            NoText::Truncated => "模型把输出额度用完了还没给出正文（常见于推理模型先长时间思考）。可以重试，或在 设置 → AI 接口 里换成非推理模型".into(),
            NoText::Refused(why) => format!("模型拒绝回答或被内容过滤拦下了：{why}"),
            NoText::Unknown(excerpt) => format!("接口返回里没有文本内容，返回的是：{excerpt}"),
        }
    }
}

fn excerpt(s: &str) -> String {
    let one_line: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = one_line.chars().take(240).collect();
    if one_line.chars().count() > 240 {
        out.push('…');
    }
    out
}

/// Text from a content value that is a string, or a list of parts like
/// `[{"type":"text","text":"…"}]` (thinking / reasoning parts are skipped).
fn content_text(c: &Value) -> String {
    match c {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| !matches!(p["type"].as_str(), Some("thinking" | "redacted_thinking" | "reasoning")))
            .filter_map(|p| p["text"].as_str().or_else(|| p.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// `openai` = Chat Completions shape, anything else = Anthropic Messages shape.
/// `body` is the raw response body.
pub fn reply_text(format: &str, body: &str) -> Result<String, NoText> {
    let trimmed = body.trim_start();
    if trimmed.starts_with("data:") || trimmed.starts_with("event:") {
        return sse_text(body);
    }
    let v: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Err(NoText::Unknown(excerpt(body))),
    };
    // a gateway may answer in the other format than the one configured
    let (text, stop) = if format == "openai" || v.get("choices").is_some() {
        let c = &v["choices"][0];
        let text = content_text(&c["message"]["content"]);
        let text = if text.is_empty() { c["text"].as_str().unwrap_or("").to_string() } else { text };
        (text, c["finish_reason"].as_str().unwrap_or("").to_string())
    } else {
        (content_text(&v["content"]), v["stop_reason"].as_str().unwrap_or("").to_string())
    };
    finish(text, &stop, &v)
}

fn finish(text: String, stop: &str, v: &Value) -> Result<String, NoText> {
    let cut = matches!(stop, "length" | "max_tokens");
    if !text.trim().is_empty() {
        return if cut { Err(NoText::Partial(text)) } else { Ok(text) };
    }
    match stop {
        "length" | "max_tokens" => Err(NoText::Truncated),
        "content_filter" | "refusal" => Err(NoText::Refused(stop.to_string())),
        _ => Err(NoText::Unknown(excerpt(&v.to_string()))),
    }
}

/// A streamed reply (server-sent events) in either format.
fn sse_text(body: &str) -> Result<String, NoText> {
    let mut text = String::new();
    let mut stop = String::new();
    for line in body.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else { continue };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        // OpenAI: choices[0].delta.content
        if let Some(c) = v["choices"].get(0) {
            text.push_str(&content_text(&c["delta"]["content"]));
            if let Some(r) = c["finish_reason"].as_str() {
                stop = r.to_string();
            }
        }
        // Anthropic: content_block_delta with a text_delta
        if v["type"] == "content_block_delta" && v["delta"]["type"] == "text_delta" {
            text.push_str(v["delta"]["text"].as_str().unwrap_or(""));
        }
        if let Some(r) = v["delta"]["stop_reason"].as_str() {
            stop = r.to_string();
        }
    }
    finish(text, &stop, &Value::String(excerpt(body)))
}
