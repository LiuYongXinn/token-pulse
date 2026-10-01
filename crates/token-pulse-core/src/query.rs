//! Query dimensions use stable opaque keys, never display labels as SQL input.
use crate::protocol::TokenTotals;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

pub fn model_key(provider: Option<&str>, model: Option<&str>) -> Option<String> {
    model.map(|name| {
        // JSON encodes null and component boundaries without separator ambiguity.
        let identity = serde_json::to_vec(&(provider, name)).expect("string pair JSON");
        format!("model:{:x}", Sha256::digest(identity))
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum GroupDimension {
    Models,
    Projects,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum GroupSort {
    TotalDesc,
    NameAsc,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct GroupedUsage {
    pub key: Option<String>,
    pub display_name: String,
    pub totals: TokenTotals,
}
