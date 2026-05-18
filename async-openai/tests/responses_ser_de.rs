use async_openai::types::responses::{EasyInputMessage, MessageType, Role};

#[test]
fn easy_input_message_type_defaults_when_missing() {
    // `type` is omitted — should default to MessageType::Message via #[serde(default)]
    let json = r#"{"role":"user","content":"hello"}"#;
    let msg: EasyInputMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.r#type, MessageType::Message);
    assert_eq!(msg.role, Role::User);

    // `type` present — should round-trip unchanged
    let json_with_type = r#"{"type":"message","role":"user","content":"hello"}"#;
    let msg2: EasyInputMessage = serde_json::from_str(json_with_type).unwrap();
    assert_eq!(msg2.r#type, MessageType::Message);
    assert_eq!(msg, msg2);
}
