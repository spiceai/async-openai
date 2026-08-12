use async_openai::types::responses::{CreateResponse, EasyInputMessage, MessageType, Role, Tool};
use serde_json::json;

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

#[test]
fn namespace_tool_round_trips() {
    let request: CreateResponse = serde_json::from_value(json!({
        "model": "test-model",
        "input": "hello",
        "tools": [{
            "type": "namespace",
            "name": "local_tools",
            "description": "Tools provided by the local runtime.",
            "tools": [{
                "type": "function",
                "name": "read_file",
                "parameters": {"type": "object"}
            }]
        }]
    }))
    .expect("namespace tool should deserialize");

    let tools = request
        .tools
        .as_ref()
        .expect("request should contain tools");
    assert!(matches!(tools.as_slice(), [Tool::Namespace(_)]));

    let value = serde_json::to_value(request).expect("request should serialize");
    assert_eq!(value["tools"][0]["type"], "namespace");
    assert_eq!(value["tools"][0]["name"], "local_tools");
}
