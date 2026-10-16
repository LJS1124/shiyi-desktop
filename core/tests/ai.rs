use shiyi_core::ai::{reply_text, NoText};

#[test]
fn anthropic_plain_and_with_thinking() {
    let b = r#"{"content":[{"type":"text","text":"你好"}],"stop_reason":"end_turn"}"#;
    assert_eq!(reply_text("anthropic", b).unwrap(), "你好");
    let b = r#"{"content":[{"type":"thinking","thinking":"嗯"},{"type":"text","text":"A"},{"type":"text","text":"B"}]}"#;
    assert_eq!(reply_text("anthropic", b).unwrap(), "AB");
}

#[test]
fn thinking_used_the_whole_budget() {
    let b = r#"{"content":[{"type":"thinking","thinking":"……"}],"stop_reason":"max_tokens"}"#;
    assert_eq!(reply_text("anthropic", b), Err(NoText::Truncated));
    let b = r#"{"choices":[{"message":{"content":null,"reasoning_content":"……"},"finish_reason":"length"}]}"#;
    assert_eq!(reply_text("openai", b), Err(NoText::Truncated));
    let b = r#"{"choices":[{"message":{"content":""},"finish_reason":"length"}]}"#;
    assert_eq!(reply_text("openai", b), Err(NoText::Truncated));
}

#[test]
fn openai_string_and_parts() {
    let b = r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}"#;
    assert_eq!(reply_text("openai", b).unwrap(), "ok");
    let b = r#"{"choices":[{"message":{"content":[{"type":"reasoning","text":"x"},{"type":"text","text":"一"},{"type":"text","text":"二"}]}}]}"#;
    assert_eq!(reply_text("openai", b).unwrap(), "一二");
}

#[test]
fn format_mismatch_is_tolerated() {
    let b = r#"{"choices":[{"message":{"content":"from openai shape"}}]}"#;
    assert_eq!(reply_text("anthropic", b).unwrap(), "from openai shape");
}

#[test]
fn streamed_replies() {
    let openai = "data: {\"choices\":[{\"delta\":{\"content\":\"拾\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"忆\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n";
    assert_eq!(reply_text("openai", openai).unwrap(), "拾忆");
    let anthropic = "event: message_start\ndata: {\"type\":\"message_start\"}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"x\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"好\"}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n";
    assert_eq!(reply_text("anthropic", anthropic).unwrap(), "好");
    let cut = "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\"}}\n";
    assert_eq!(reply_text("anthropic", cut), Err(NoText::Truncated));
}

#[test]
fn refusal_and_unknown_say_what_came_back() {
    let b = r#"{"choices":[{"message":{"content":""},"finish_reason":"content_filter"}]}"#;
    assert!(matches!(reply_text("openai", b), Err(NoText::Refused(_))));
    let b = r#"{"result":"weird"}"#;
    match reply_text("anthropic", b) {
        Err(NoText::Unknown(e)) => assert!(e.contains("weird")),
        other => panic!("{other:?}"),
    }
    match reply_text("openai", "<html>gateway error</html>") {
        Err(e) => assert!(e.message().contains("gateway error")),
        Ok(_) => panic!(),
    }
}
