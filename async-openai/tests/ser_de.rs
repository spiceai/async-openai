use async_openai::types::chat::{
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    ChatCompletionStreamOptions, CreateChatCompletionRequest, CreateChatCompletionRequestArgs,
};

#[test]
fn chat_types_serde() {
    let request: CreateChatCompletionRequest = CreateChatCompletionRequestArgs::default()
        .messages([
            ChatCompletionRequestSystemMessageArgs::default()
                .content("your are a calculator")
                .build()
                .unwrap()
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content("what is the result of 1+1")
                .build()
                .unwrap()
                .into(),
        ])
        .build()
        .unwrap();
    // serialize the request
    let serialized = serde_json::to_string(&request).unwrap();
    // deserialize the request
    let deserialized: CreateChatCompletionRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(request, deserialized);
}

#[test]
fn stream_options_none_fields_not_serialized() {
    // When include_obfuscation is None, it should not appear in the serialized JSON.
    // This is important for OpenAI-compatible providers (like NVIDIA NIM) that reject unknown fields.
    let stream_options = ChatCompletionStreamOptions {
        include_usage: Some(true),
        include_obfuscation: None,
    };

    let serialized = serde_json::to_string(&stream_options).unwrap();

    // Verify include_usage is present
    assert!(serialized.contains("include_usage"));
    // Verify include_obfuscation is NOT present (not even as null)
    assert!(
        !serialized.contains("include_obfuscation"),
        "include_obfuscation should not be serialized when None, but got: {}",
        serialized
    );

    // Test when both are None
    let stream_options_empty = ChatCompletionStreamOptions {
        include_usage: None,
        include_obfuscation: None,
    };

    let serialized_empty = serde_json::to_string(&stream_options_empty).unwrap();
    assert_eq!(serialized_empty, "{}");

    // Test roundtrip deserialization
    let deserialized: ChatCompletionStreamOptions = serde_json::from_str(&serialized).unwrap();
    assert_eq!(stream_options, deserialized);
}

#[test]
fn allowed_tools_tool_choice_round_trips_in_the_chat_completions_shape() {
    use async_openai::types::chat::{ChatCompletionToolChoiceOption, ToolChoiceAllowedMode};

    // The Chat Completions API takes one `allowed_tools` object, not a list of them.
    let wire = serde_json::json!({
        "type": "allowed_tools",
        "allowed_tools": {
            "mode": "required",
            "tools": [{"type": "function", "function": {"name": "get_weather"}}]
        }
    });

    let choice: ChatCompletionToolChoiceOption = serde_json::from_value(wire.clone()).unwrap();
    let ChatCompletionToolChoiceOption::AllowedTools(allowed) = &choice else {
        panic!("expected an allowed_tools choice, got {choice:?}");
    };
    assert_eq!(allowed.allowed_tools.mode, ToolChoiceAllowedMode::Required);
    assert_eq!(
        allowed.allowed_tools.tools,
        vec![serde_json::json!({"type": "function", "function": {"name": "get_weather"}})]
    );
    assert_eq!(serde_json::to_value(&choice).unwrap(), wire);

    let list = serde_json::json!({
        "type": "allowed_tools",
        "allowed_tools": [{"mode": "required", "tools": []}]
    });
    assert!(
        serde_json::from_value::<ChatCompletionToolChoiceOption>(list).is_err(),
        "a list of allowed_tools entries is not the Chat Completions shape"
    );
}
