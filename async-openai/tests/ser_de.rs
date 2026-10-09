use async_openai::types::chat::{
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    ChatCompletionStreamOptions, CreateChatCompletionRequest, CreateChatCompletionRequestArgs,
    FunctionCallStream,
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
fn function_call_stream_none_fields_not_serialized() {
    // When name or arguments is None, it should not appear in the serialized JSON.
    // Streaming consumers that read function.arguments with a string default
    // (e.g. `dict.get('arguments', '')`) crash on explicit JSON null because the
    // key is present-but-null rather than absent.
    let fcs = FunctionCallStream {
        name: Some("get_weather".to_string()),
        arguments: None,
    };

    let serialized = serde_json::to_string(&fcs).unwrap();

    // Verify name is present
    assert!(serialized.contains("name"));
    // Verify arguments is NOT present (not even as null)
    assert!(
        !serialized.contains("arguments"),
        "arguments should not be serialized when None, but got: {}",
        serialized
    );

    // Test when both are None
    let fcs_empty = FunctionCallStream {
        name: None,
        arguments: None,
    };

    let serialized_empty = serde_json::to_string(&fcs_empty).unwrap();
    assert_eq!(serialized_empty, "{}");

    // Test roundtrip deserialization
    let deserialized: FunctionCallStream = serde_json::from_str(&serialized).unwrap();
    assert_eq!(fcs, deserialized);
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

mod service_tier {
    use async_openai::types::chat::{
        CreateChatCompletionRequest, CreateChatCompletionResponse, ServiceTier, ServiceTierResponse,
    };
    use serde_json::{json, Value};
    use utoipa::PartialSchema;

    fn response_with_tier(tier: &str) -> Value {
        json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1_755_639_134,
            "model": "gpt-4o-mini",
            "service_tier": tier,
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop",
                "logprobs": null
            }]
        })
    }

    #[test]
    fn a_response_with_an_unnamed_tier_deserializes_and_keeps_it() {
        let response: CreateChatCompletionResponse =
            serde_json::from_value(response_with_tier("fast")).unwrap();
        assert_eq!(
            response.service_tier,
            Some(ServiceTierResponse::Other("fast".to_string()))
        );
        assert_eq!(
            serde_json::to_value(&response).unwrap()["service_tier"],
            json!("fast")
        );
    }

    #[test]
    fn a_named_tier_deserializes_to_its_variant() {
        let response: CreateChatCompletionResponse =
            serde_json::from_value(response_with_tier("priority")).unwrap();
        assert_eq!(response.service_tier, Some(ServiceTierResponse::Priority));
    }

    #[test]
    fn a_request_names_only_the_closed_tiers() {
        let request = |tier: &str| {
            json!({
                "model": "gpt-4o-mini",
                "messages": [{"role": "user", "content": "hello"}],
                "service_tier": tier
            })
        };
        let named: CreateChatCompletionRequest =
            serde_json::from_value(request("priority")).unwrap();
        assert_eq!(named.service_tier, Some(ServiceTier::Priority));
        serde_json::from_value::<CreateChatCompletionRequest>(request("fast")).unwrap_err();
    }

    #[test]
    fn a_tier_that_is_not_a_string_is_refused() {
        serde_json::from_value::<ServiceTierResponse>(json!(1)).unwrap_err();
    }

    #[test]
    fn the_schema_is_any_string_and_lists_exactly_the_named_tiers() {
        let schema = serde_json::to_value(ServiceTierResponse::schema()).unwrap();
        let any_of = schema["anyOf"].as_array().unwrap();
        assert_eq!(any_of.len(), 2, "{schema}");
        assert_eq!(any_of[1], json!({"type": "string"}), "{schema}");

        let named: Vec<&str> = any_of[0]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(named, ["auto", "default", "flex", "scale", "priority"]);
        for name in named {
            let tier: ServiceTierResponse = serde_json::from_value(json!(name)).unwrap();
            assert!(
                !matches!(tier, ServiceTierResponse::Other(_)),
                "`{name}` is listed as a named tier but deserializes to `Other`"
            );
        }
    }
}
