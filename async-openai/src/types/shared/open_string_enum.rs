//! The schema of an enum that keeps a value it does not name verbatim in an untagged
//! `Other(String)`, which the derive cannot describe.

/// The schema of an enum that keeps a value it does not name verbatim in an untagged
/// `Other(String)`: any string, listing the named values. Hand-written because the derive
/// ignores `#[serde(untagged)]` on a variant and would describe `Other` as an object.
pub(crate) fn open_string_enum_schema(
    named: &'static [&'static str],
) -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{AnyOfBuilder, ObjectBuilder, Type};

    AnyOfBuilder::new()
        .item(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .enum_values(Some(named.iter().copied())),
        )
        .item(ObjectBuilder::new().schema_type(Type::String))
        .into()
}
