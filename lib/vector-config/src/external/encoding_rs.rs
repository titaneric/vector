use crate::{
    schema::generate_string_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError, Metadata,
};

impl Configurable for &'static encoding_rs::Encoding {
    // TODO: At some point, we might want to override the metadata to define a validation pattern that only matches
    // valid character set encodings... but that would be a very large array of strings, and technically the Encoding
    // Standard is a living standard, so... :thinkies:

    fn referenceable_name() -> Option<&'static str> {
        Some("encoding_rs::Encoding")
    }

    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        metadata.set_description(
            "An encoding as defined in the [Encoding Standard](https://encoding.spec.whatwg.org/).",
        );
        metadata
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}
