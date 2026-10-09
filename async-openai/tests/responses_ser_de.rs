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

mod service_tier {
    use async_openai::types::responses::{
        CreateResponse, Response, ServiceTier, ServiceTierResponse,
    };
    use serde_json::{json, Value};
    use utoipa::PartialSchema;

    fn response_with_tier(tier: &str) -> Value {
        json!({
            "id": "resp_1",
            "object": "response",
            "created_at": 1_755_639_134,
            "model": "gpt-4o-mini",
            "service_tier": tier,
            "status": "completed",
            "output": []
        })
    }

    #[test]
    fn a_response_with_an_unnamed_tier_deserializes_and_keeps_it() {
        let response: Response = serde_json::from_value(response_with_tier("fast")).unwrap();
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
        let response: Response = serde_json::from_value(response_with_tier("priority")).unwrap();
        assert_eq!(response.service_tier, Some(ServiceTierResponse::Priority));
    }

    #[test]
    fn a_request_names_only_the_closed_tiers() {
        let request = |tier: &str| {
            json!({
                "model": "gpt-4o-mini",
                "input": "hello",
                "service_tier": tier
            })
        };
        let named: CreateResponse = serde_json::from_value(request("priority")).unwrap();
        assert_eq!(named.service_tier, Some(ServiceTier::Priority));
        serde_json::from_value::<CreateResponse>(request("fast")).unwrap_err();
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
