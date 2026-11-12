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
        CreateResponse, Response, ServiceTierResponse, ServiceTierResponses,
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
        for (name, tier) in [
            ("priority", ServiceTierResponses::Priority),
            ("fast", ServiceTierResponses::Fast),
            ("ultrafast", ServiceTierResponses::Ultrafast),
        ] {
            let named: CreateResponse = serde_json::from_value(request(name)).unwrap();
            assert_eq!(named.service_tier, Some(tier), "{name}");
        }
        let error = serde_json::from_value::<CreateResponse>(request("turbo"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown variant `turbo`"), "{error}");
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

/// The schemas these types publish must accept what their serde implementation accepts. The
/// derive ignores `#[serde(untagged)]` on a variant, so without hand-written schemas it would
/// require a `type` naming the Rust variant on `InputItem`'s untagged alternatives, and describe
/// an `Other(String)` fallback as an object.
mod schemas_accept_what_serde_accepts {
    use async_openai::types::responses::{
        CompoundType, InputItem, MisalignmentErrorType, Reasoning, ReasoningModeEnum,
        ResponseError, ResponseErrorCode,
    };
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};
    use utoipa::{OpenApi, PartialSchema, ToSchema};

    // `CompoundType` is listed by hand: `CompoundFilter`'s `no_recursion` stops it from
    // registering its dependencies, and the validator needs every reference to resolve.
    #[derive(OpenApi)]
    #[openapi(components(schemas(InputItem, ResponseError, Reasoning, CompoundType)))]
    struct Document;

    fn validator<T: ToSchema>() -> jsonschema::Validator {
        let mut document = serde_json::to_value(Document::openapi()).unwrap();
        document["$ref"] = json!(format!("#/components/schemas/{}", T::name()));
        jsonschema::validator_for(&document).unwrap()
    }

    fn assert_accepted<T: ToSchema + DeserializeOwned + std::fmt::Debug>(
        value: Value,
        is_expected: impl Fn(&T) -> bool,
    ) {
        let parsed: T = serde_json::from_value(value.clone()).unwrap();
        assert!(is_expected(&parsed), "{value} parsed as {parsed:?}");
        let errors: Vec<String> = validator::<T>()
            .iter_errors(&value)
            .map(|error| error.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "{value} rejected by the schema: {errors:?}"
        );
    }

    #[test]
    fn every_input_item_alternative_validates() {
        let reference = |item: &InputItem| matches!(item, InputItem::ItemReference(_));
        assert_accepted(json!({"id": "msg_1"}), reference);
        assert_accepted(json!({"type": "item_reference", "id": "msg_1"}), reference);
        assert_accepted(
            json!({"role": "user", "content": "hello"}),
            |item: &InputItem| matches!(item, InputItem::EasyMessage(_)),
        );
        assert_accepted(
            json!({"type": "function_call_output", "call_id": "call_1", "output": "ok"}),
            |item: &InputItem| matches!(item, InputItem::Item(_)),
        );
        assert_accepted(json!({"type": "compaction_trigger"}), |item: &InputItem| {
            matches!(item, InputItem::CompactionTrigger(_))
        });
        assert_accepted(
            json!({"type": "program", "id": "p_1", "call_id": "c_1", "code": "1", "fingerprint": "f"}),
            |item: &InputItem| matches!(item, InputItem::Program(_)),
        );
        assert_accepted(
            json!({"type": "program_output", "id": "po_1", "call_id": "c_1", "result": "ok", "status": "completed"}),
            |item: &InputItem| matches!(item, InputItem::ProgramOutput(_)),
        );
    }

    #[test]
    fn a_tagged_input_item_must_name_its_own_tag() {
        let value = json!({"type": "program", "id": "po_1", "call_id": "c_1", "result": "ok", "status": "completed"});
        serde_json::from_value::<InputItem>(value.clone()).unwrap_err();
        assert!(!validator::<InputItem>().is_valid(&value), "{value}");
    }

    #[test]
    fn an_error_code_the_enum_does_not_name_validates() {
        let error = |code: &str| json!({"code": code, "message": "failed"});
        assert_accepted(error("server_error"), |error: &ResponseError| {
            error.code == ResponseErrorCode::ServerError
        });
        assert_accepted(error("a_new_code"), |error: &ResponseError| {
            error.code == ResponseErrorCode::Other("a_new_code".to_string())
        });
        let not_a_string = json!({"code": 1, "message": "failed"});
        serde_json::from_value::<ResponseError>(not_a_string.clone()).unwrap_err();
        assert!(!validator::<ResponseError>().is_valid(&not_a_string));
    }

    #[test]
    fn a_misalignment_classification_the_enum_does_not_name_validates() {
        let error = |error_type: Value| {
            json!({
                "code": "misalignment_policy_violation",
                "message": "blocked",
                "misalignment": {"error_type": error_type},
            })
        };
        let classified = |expected: MisalignmentErrorType| {
            move |error: &ResponseError| {
                error
                    .misalignment
                    .as_ref()
                    .and_then(|misalignment| misalignment.error_type.as_ref())
                    == Some(&expected)
            }
        };
        assert_accepted(
            error(json!("potentially_unintended_data_transfer")),
            classified(MisalignmentErrorType::PotentiallyUnintendedDataTransfer),
        );
        assert_accepted(
            error(json!("a_new_classification")),
            classified(MisalignmentErrorType::String(
                "a_new_classification".to_string(),
            )),
        );
        let not_a_string = error(json!(1));
        serde_json::from_value::<ResponseError>(not_a_string.clone()).unwrap_err();
        assert!(!validator::<ResponseError>().is_valid(&not_a_string));
    }

    #[test]
    fn a_reasoning_mode_the_enum_does_not_name_validates() {
        assert_accepted(json!({"mode": "pro"}), |reasoning: &Reasoning| {
            reasoning.mode == Some(ReasoningModeEnum::Pro)
        });
        assert_accepted(json!({"mode": "max"}), |reasoning: &Reasoning| {
            reasoning.mode == Some(ReasoningModeEnum::Other("max".to_string()))
        });
    }

    /// Every value listed as a named code or mode deserializes to its named variant, not `Other`.
    #[test]
    fn the_listed_names_are_the_named_variants() {
        fn listed<T: PartialSchema>() -> Vec<String> {
            let schema = serde_json::to_value(T::schema()).unwrap();
            let any_of = schema["anyOf"].as_array().unwrap();
            assert_eq!(any_of.len(), 2, "{schema}");
            assert_eq!(any_of[1], json!({"type": "string"}), "{schema}");
            any_of[0]["enum"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_string())
                .collect()
        }

        let codes = listed::<ResponseErrorCode>();
        assert_eq!(codes.len(), 21, "{codes:?}");
        for code in codes {
            let parsed: ResponseErrorCode = serde_json::from_value(json!(code)).unwrap();
            assert!(!matches!(parsed, ResponseErrorCode::Other(_)), "{code}");
        }
        assert_eq!(listed::<ReasoningModeEnum>(), ["pro", "standard"]);
        let classifications = listed::<MisalignmentErrorType>();
        assert_eq!(classifications.len(), 4, "{classifications:?}");
        for classification in classifications {
            let parsed: MisalignmentErrorType =
                serde_json::from_value(json!(classification)).unwrap();
            assert!(
                !matches!(parsed, MisalignmentErrorType::String(_)),
                "{classification}"
            );
        }
    }
}
