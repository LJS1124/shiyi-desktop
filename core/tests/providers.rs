use shiyi_core::providers::{catalog_json, get, guess, is_allowed_external, key_hint, models_url, parse_models, CATALOG};
use std::collections::HashSet;

#[test]
fn catalog_has_twelve_unique_providers() {
    assert_eq!(CATALOG.len(), 12);
    let ids: HashSet<&str> = CATALOG.iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), CATALOG.len(), "provider ids must be unique");
}

#[test]
fn catalog_bases_and_key_urls() {
    for p in CATALOG.iter().filter(|p| p.id != "custom") {
        assert!(!p.base.is_empty(), "{} has an empty base", p.id);
        assert!(
            p.base.starts_with("https://") || p.base.starts_with("http://"),
            "{} base is not http(s): {}",
            p.id,
            p.base
        );
        assert!(!p.key_url.is_empty(), "{} has an empty key_url", p.id);
        assert!(p.key_url.starts_with("https://"), "{} key_url is not https: {}", p.id, p.key_url);
    }
}

#[test]
fn only_ollama_skips_the_key() {
    for p in CATALOG {
        if p.id == "ollama" {
            assert!(!p.needs_key, "ollama must not need a key");
        } else {
            assert!(p.needs_key, "{} must need a key", p.id);
        }
    }
}

#[test]
fn formats_are_anthropic_or_openai() {
    for p in CATALOG {
        assert!(matches!(p.format, "anthropic" | "openai"), "{} has format {:?}", p.id, p.format);
    }
}

#[test]
fn get_finds_by_id() {
    let p = get("deepseek").expect("deepseek is in the catalog");
    assert_eq!(p.name, "DeepSeek");
    assert!(get("nope").is_none());
}

#[test]
fn catalog_json_shape() {
    let v = catalog_json();
    let arr = v.as_array().expect("catalog_json is an array");
    assert_eq!(arr.len(), 12);
    let keys = ["id", "name", "format", "base", "keyUrl", "needsKey", "color", "models", "region"];
    for item in arr {
        let obj = item.as_object().expect("each entry is an object");
        for k in keys {
            assert!(obj.contains_key(k), "missing key {k} in {item}");
        }
        assert_eq!(obj.len(), keys.len(), "unexpected extra keys in {item}");
        assert!(obj["models"].is_array(), "models must be an array in {item}");
    }
}

#[test]
fn guess_provider_from_old_settings() {
    assert_eq!(guess("", "anthropic"), "anthropic");
    assert_eq!(guess("", "openai"), "openai");
    assert_eq!(guess("https://api.deepseek.com/anthropic", "anthropic"), "deepseek");
    assert_eq!(guess("https://api.deepseek.com", "openai"), "deepseek");
    assert_eq!(guess("https://API.MOONSHOT.CN/v1", "openai"), "moonshot");
    assert_eq!(guess("https://dashscope.aliyuncs.com/compatible-mode/v1", "openai"), "dashscope");
    assert_eq!(guess("http://localhost:11434/v1", "openai"), "ollama");
    assert_eq!(guess("https://my-gateway.example.com/v1", "openai"), "custom");
    assert_eq!(guess("not a url", "openai"), "custom");
}

#[test]
fn key_hint_masks_keys() {
    assert_eq!(key_hint(""), "");
    assert_eq!(key_hint("   "), "");
    assert_eq!(key_hint("short"), "••••rt");
    assert_eq!(key_hint("12345678"), "••••78");
    assert_eq!(key_hint("sk-1234567890ab"), "••••ab");
    assert_eq!(key_hint("sk-abcdefghijklmn1234"), "sk-••••1234");
    assert_eq!(key_hint("  sk-abcdefghijklmn1234  "), "sk-••••1234");
}

#[test]
fn models_url_variants() {
    assert_eq!(models_url("openai", "https://api.openai.com/v1"), "https://api.openai.com/v1/models");
    assert_eq!(models_url("openai", "https://api.openai.com/v1/"), "https://api.openai.com/v1/models");
    assert_eq!(models_url("anthropic", ""), "https://api.anthropic.com/v1/models");
    assert_eq!(models_url("anthropic", "https://x.com/v1"), "https://x.com/v1/models");
    assert_eq!(models_url("anthropic", "https://x.com"), "https://x.com/v1/models");
}

#[test]
fn parse_models_openai_shape_sorted_and_deduped() {
    assert_eq!(parse_models(r#"{"data":[{"id":"b"},{"id":"a"},{"id":"a"}]}"#), vec!["a", "b"]);
}

#[test]
fn parse_models_anthropic_shape() {
    let body = r#"{"data":[
        {"type":"model","id":"claude-b","display_name":"Claude B"},
        {"type":"model","id":"claude-a","display_name":"Claude A"}
    ]}"#;
    assert_eq!(parse_models(body), vec!["claude-a", "claude-b"]);
}

#[test]
fn parse_models_gemini_strips_models_prefix() {
    assert_eq!(parse_models(r#"{"models":[{"name":"models/gemini-x"}]}"#), vec!["gemini-x"]);
}

#[test]
fn parse_models_models_key_with_name() {
    assert_eq!(parse_models(r#"{"models":[{"name":"llama3"}]}"#), vec!["llama3"]);
}

#[test]
fn parse_models_bare_array() {
    assert_eq!(parse_models(r#"["x","y"]"#), vec!["x", "y"]);
}

#[test]
fn parse_models_invalid_json_is_empty() {
    assert!(parse_models("not json").is_empty());
    assert!(parse_models("").is_empty());
}

#[test]
fn parse_models_drops_empty_ids() {
    assert_eq!(parse_models(r#"{"data":[{"id":""},{"id":"a"},{"name":""},{"id":"models/"}]}"#), vec!["a"]);
}

#[test]
fn allowed_external_key_pages() {
    assert!(is_allowed_external("https://platform.deepseek.com/api_keys"));
    assert!(is_allowed_external("https://console.anthropic.com/settings/keys"));
    assert!(is_allowed_external("https://openrouter.ai/keys"));
}

#[test]
fn allowed_external_rejects_unsafe_urls() {
    assert!(!is_allowed_external("http://platform.deepseek.com/api_keys"));
    assert!(!is_allowed_external("http://console.anthropic.com/settings/keys"));
    assert!(!is_allowed_external("http://openrouter.ai/keys"));
    assert!(!is_allowed_external("https://evil.example.com/"));
    assert!(!is_allowed_external("https://platform.deepseek.com.evil.com/"));
    assert!(!is_allowed_external("https://user@platform.deepseek.com/"));
    assert!(!is_allowed_external("https://platform.deepseek.com/../api_keys"));
    assert!(!is_allowed_external("https://platform.deepseek.com/api keys"));
}

#[test]
fn guess_knows_other_regions() {
    use shiyi_core::providers::guess;
    assert_eq!(guess("https://dashscope-intl.aliyuncs.com/compatible-mode/v1", "openai"), "dashscope");
    assert_eq!(guess("https://api.moonshot.ai/v1", "openai"), "moonshot");
}
