//! Model providers the built-in AI can use: one catalog for the settings page
//! (names, defaults, where to get a key), migrating old settings, and the
//! allowlist of pages the app may open in the browser.

use serde_json::{json, Value};

pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    /// `anthropic` (Messages API) or `openai` (Chat Completions)
    pub format: &'static str,
    pub base: &'static str,
    /// where to create an API key
    pub key_url: &'static str,
    pub needs_key: bool,
    /// brand colour for the letter badge when no icon is available
    pub color: &'static str,
    /// suggested models (main, quick); empty = pick from the fetched list
    pub models: &'static [&'static str],
    pub region: &'static str,
}

pub const CATALOG: &[Provider] = &[
    Provider { id: "anthropic", name: "Anthropic", format: "anthropic", base: "https://api.anthropic.com", key_url: "https://console.anthropic.com/settings/keys", needs_key: true, color: "#D97757", models: &["claude-sonnet-5-5", "claude-haiku-5-5"], region: "global" },
    Provider { id: "openai", name: "OpenAI", format: "openai", base: "https://api.openai.com/v1", key_url: "https://platform.openai.com/api-keys", needs_key: true, color: "#10A37F", models: &[], region: "global" },
    Provider { id: "gemini", name: "Google Gemini", format: "openai", base: "https://generativelanguage.googleapis.com/v1beta/openai", key_url: "https://aistudio.google.com/apikey", needs_key: true, color: "#4285F4", models: &[], region: "global" },
    Provider { id: "openrouter", name: "OpenRouter", format: "openai", base: "https://openrouter.ai/api/v1", key_url: "https://openrouter.ai/keys", needs_key: true, color: "#6467F2", models: &[], region: "global" },
    Provider { id: "deepseek", name: "DeepSeek", format: "openai", base: "https://api.deepseek.com", key_url: "https://platform.deepseek.com/api_keys", needs_key: true, color: "#4D6BFE", models: &["deepseek-chat", "deepseek-reasoner"], region: "cn" },
    Provider { id: "dashscope", name: "阿里云百炼", format: "openai", base: "https://dashscope.aliyuncs.com/compatible-mode/v1", key_url: "https://bailian.console.aliyun.com/", needs_key: true, color: "#615CED", models: &["qwen-plus", "qwen-turbo"], region: "cn" },
    Provider { id: "moonshot", name: "月之暗面 Kimi", format: "openai", base: "https://api.moonshot.cn/v1", key_url: "https://platform.moonshot.cn/console/api-keys", needs_key: true, color: "#16191E", models: &[], region: "cn" },
    Provider { id: "zhipu", name: "智谱 GLM", format: "openai", base: "https://open.bigmodel.cn/api/paas/v4", key_url: "https://open.bigmodel.cn/", needs_key: true, color: "#3859FF", models: &[], region: "cn" },
    Provider { id: "ark", name: "火山方舟 豆包", format: "openai", base: "https://ark.cn-beijing.volces.com/api/v3", key_url: "https://console.volcengine.com/ark", needs_key: true, color: "#1E6FFF", models: &[], region: "cn" },
    Provider { id: "siliconflow", name: "硅基流动", format: "openai", base: "https://api.siliconflow.cn/v1", key_url: "https://cloud.siliconflow.cn/account/ak", needs_key: true, color: "#7C3AED", models: &[], region: "cn" },
    Provider { id: "ollama", name: "Ollama 本地", format: "openai", base: "http://localhost:11434/v1", key_url: "https://ollama.com/download", needs_key: false, color: "#111111", models: &[], region: "local" },
    Provider { id: "custom", name: "自定义", format: "openai", base: "", key_url: "", needs_key: true, color: "#8E8E93", models: &[], region: "local" },
];

pub fn get(id: &str) -> Option<&'static Provider> {
    CATALOG.iter().find(|p| p.id == id)
}

/// The catalog as JSON for the settings page.
pub fn catalog_json() -> Value {
    Value::Array(
        CATALOG
            .iter()
            .map(|p| json!({ "id": p.id, "name": p.name, "format": p.format, "base": p.base, "keyUrl": p.key_url, "needsKey": p.needs_key, "color": p.color, "models": p.models, "region": p.region }))
            .collect(),
    )
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?; // never trust credentials in a URL
    Some(host.split(':').next()?.to_lowercase())
}

/// Which provider an older single-provider setup (format + address) was using.
pub fn guess(api_base: &str, api_format: &str) -> &'static str {
    let base = api_base.trim();
    if base.is_empty() {
        return if api_format == "openai" { "openai" } else { "anthropic" };
    }
    let Some(host) = host_of(base) else { return "custom" };
    // other regions of the same service
    const ALIASES: [(&str, &str); 2] = [("dashscope-intl.aliyuncs.com", "dashscope"), ("api.moonshot.ai", "moonshot")];
    if let Some((_, id)) = ALIASES.iter().find(|(h, _)| *h == host) {
        return id;
    }
    CATALOG
        .iter()
        .filter(|p| !p.base.is_empty())
        .find(|p| host_of(p.base).as_deref() == Some(host.as_str()))
        .map(|p| p.id)
        .unwrap_or("custom")
}

/// `sk-abcd…wxyz` → `sk-••••wxyz`; short keys show only dots.
pub fn key_hint(key: &str) -> String {
    let k = key.trim();
    if k.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = k.chars().collect();
    // short keys: showing 3 + 4 characters would give most of it away
    if chars.len() <= 16 {
        return format!("••••{}", chars[chars.len().saturating_sub(2)..].iter().collect::<String>());
    }
    let head: String = chars.iter().take(3).collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}••••{tail}")
}

/// Where to list a provider's models.
pub fn models_url(format: &str, base: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if format == "anthropic" {
        let b = if b.is_empty() { "https://api.anthropic.com" } else { b };
        if b.ends_with("/v1") { format!("{b}/models") } else { format!("{b}/v1/models") }
    } else {
        format!("{b}/models")
    }
}

/// Model ids from a `/models` response (OpenAI `{data:[{id}]}`, Anthropic
/// `{data:[{id, display_name}]}`, Gemini `models/…` ids, or a bare array).
pub fn parse_models(body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return vec![] };
    let list = v.get("data").or_else(|| v.get("models")).unwrap_or(&v);
    let mut out: Vec<String> = list
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("id").or_else(|| m.get("name")).and_then(Value::as_str).or_else(|| m.as_str()))
                .map(|s| s.trim_start_matches("models/").to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

/// Pages the app may open in the browser for the user: the key pages of the
/// providers above (https only, exact host).
pub fn is_allowed_external(url: &str) -> bool {
    if !url.starts_with("https://") || url.contains("..") || url.contains('@') || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some(host) = host_of(url) else { return false };
    CATALOG.iter().filter(|p| !p.key_url.is_empty()).any(|p| host_of(p.key_url).as_deref() == Some(host.as_str()))
}
