use base64::engine::{general_purpose, Engine};
use derive_builder::Builder;
use serde::{Deserialize, Serialize};

use crate::error::OpenAIError;

#[derive(Debug, Serialize, Clone, PartialEq, Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum EmbeddingInput {
    String(String),
    StringArray(Vec<String>),
    // Minimum value is 0, maximum value is 100257 (inclusive).
    IntegerArray(Vec<u32>),
    ArrayOfIntegerArray(Vec<Vec<u32>>),
}

#[derive(Debug, Serialize, Default, Clone, PartialEq, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EncodingFormat {
    #[default]
    Float,
    Base64,
}

#[derive(Debug, Serialize, Default, Clone, Builder, PartialEq, Deserialize, utoipa::ToSchema)]
#[builder(name = "CreateEmbeddingRequestArgs")]
#[builder(pattern = "mutable")]
#[builder(setter(into, strip_option), default)]
#[builder(derive(Debug))]
#[builder(build_fn(error = "OpenAIError"))]
pub struct CreateEmbeddingRequest {
    /// ID of the model to use. You can use the
    /// [List models](https://platform.openai.com/docs/api-reference/models/list)
    /// API to see all of your available models, or see our
    /// [Model overview](https://platform.openai.com/docs/models/overview)
    /// for descriptions of them.
    pub model: String,

    ///  Input text to embed, encoded as a string or array of tokens. To embed multiple inputs in a single request, pass an array of strings or array of token arrays. The input must not exceed the max input tokens for the model (8192 tokens for `text-embedding-ada-002`), cannot be an empty string, and any array must be 2048 dimensions or less. [Example Python code](https://cookbook.openai.com/examples/how_to_count_tokens_with_tiktoken) for counting tokens.
    pub input: EmbeddingInput,

    /// The format to return the embeddings in. Can be either `float` or [`base64`](https://pypi.org/project/pybase64/). Defaults to float
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<EncodingFormat>,

    /// A unique identifier representing your end-user, which will help OpenAI
    ///  to monitor and detect abuse. [Learn more](https://platform.openai.com/docs/usage-policies/end-user-ids).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// The number of dimensions the resulting output embeddings should have. Only supported in `text-embedding-3` and later models.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, utoipa::ToSchema)]
#[serde(untagged)]
pub enum EmbeddingVector {
    Float(Vec<f32>),
    Base64(String),
}

impl From<EmbeddingVector> for Vec<f32> {
    fn from(val: EmbeddingVector) -> Self {
        match val {
            EmbeddingVector::Float(v) => v,
            EmbeddingVector::Base64(s) => {
                let bytes = general_purpose::STANDARD
                    .decode(s)
                    .expect("openai base64 encoding to be valid");
                let chunks = bytes.chunks_exact(4);
                chunks
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect::<Vec<f32>>()
            }
        }
    }
}

/// Converts an embedding vector to a base64-encoded string.
impl From<EmbeddingVector> for String {
    fn from(val: EmbeddingVector) -> Self {
        match val {
            EmbeddingVector::Float(v) => {
                let mut bytes = Vec::with_capacity(v.len() * 4);
                for f in v {
                    bytes.extend_from_slice(&f.to_le_bytes());
                }
                general_purpose::STANDARD.encode(&bytes)
            }
            EmbeddingVector::Base64(s) => s,
        }
    }
}

impl EmbeddingVector {
    pub fn is_empty(&self) -> bool {
        match self {
            EmbeddingVector::Float(v) => v.is_empty(),

            // Don't use .len() to avoid decoding the base64 string
            EmbeddingVector::Base64(v) => v.is_empty(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            EmbeddingVector::Float(v) => v.len(),
            EmbeddingVector::Base64(v) => {
                let bytes = general_purpose::STANDARD
                    .decode(v)
                    .expect("openai base64 encoding to be valid");
                bytes.len() / 4
            }
        }
    }
}

/// Represents an embedding vector returned by embedding endpoint.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, utoipa::ToSchema)]
pub struct Embedding {
    /// The index of the embedding in the list of embeddings.
    pub index: u32,
    /// The object type, which is always "embedding".
    pub object: String,
    /// The embedding vector, which is a list of floats. The length of vector
    /// depends on the model as listed in the [embedding guide](https://platform.openai.com/docs/guides/embeddings).
    pub embedding: EmbeddingVector,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, utoipa::ToSchema)]
pub struct EmbeddingUsage {
    /// The number of tokens used by the prompt.
    pub prompt_tokens: u32,
    /// The total number of tokens used by the request.
    pub total_tokens: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct CreateEmbeddingResponse {
    pub object: String,
    /// The name of the model used to generate the embedding.
    pub model: String,
    /// The list of embeddings generated by the model.
    pub data: Vec<Embedding>,
    /// The usage information for the request.
    pub usage: EmbeddingUsage,
}
