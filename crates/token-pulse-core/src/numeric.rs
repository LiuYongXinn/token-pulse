//! Lossless IPC numbers and checked fixed point. No floating-point accounting.
use crate::error::ErrorCode;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};
use std::borrow::Cow;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(transparent)]
#[ts(type = "string")]
pub struct DecimalInt(String);

impl DecimalInt {
    pub fn parse(value: &str) -> Result<Self, ErrorCode> {
        if value.is_empty()
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(ErrorCode::InvalidQuery);
        }
        value
            .parse::<i128>()
            .map_err(|_| ErrorCode::NumericOverflow)?;
        Ok(Self(value.into()))
    }
    pub fn from_nonnegative(value: i128) -> Result<Self, ErrorCode> {
        if value < 0 {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(Self(value.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn value(&self) -> i128 {
        self.0.parse().expect("validated decimal")
    }
}
impl<'de> Deserialize<'de> for DecimalInt {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
impl JsonSchema for DecimalInt {
    fn schema_name() -> Cow<'static, str> {
        "DecimalInt".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"string","pattern":"^(0|[1-9][0-9]*)$","maxLength":39})
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(transparent)]
#[ts(type = "string")]
pub struct DecimalMoney(String);

impl DecimalMoney {
    pub fn parse(value: &str) -> Result<Self, ErrorCode> {
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        DecimalInt::parse(whole)?;
        if value.ends_with('.')
            || fraction.len() > 15
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(ErrorCode::InvalidQuery);
        }
        let padded = format!("{whole}{fraction:0<15}");
        padded
            .parse::<i128>()
            .map_err(|_| ErrorCode::NumericOverflow)?;
        Ok(Self(value.into()))
    }
    pub fn from_atoms(atoms: i128) -> Result<Self, ErrorCode> {
        if atoms < 0 {
            return Err(ErrorCode::InvalidQuery);
        }
        Self::parse(&format!(
            "{}.{:015}",
            atoms / 1_000_000_000_000_000_i128,
            atoms % 1_000_000_000_000_000_i128
        ))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for DecimalMoney {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
impl JsonSchema for DecimalMoney {
    fn schema_name() -> Cow<'static, str> {
        "DecimalMoney".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"string","pattern":"^(0|[1-9][0-9]*)(\\.[0-9]{1,15})?$","maxLength":40})
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(transparent)]
#[ts(type = "number")]
pub struct EpochMs(i64);
impl EpochMs {
    pub fn new(value: i64) -> Result<Self, ErrorCode> {
        if !(-8_640_000_000_000_000..=8_640_000_000_000_000).contains(&value) {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(Self(value))
    }
    pub fn value(self) -> i64 {
        self.0
    }
}
impl<'de> Deserialize<'de> for EpochMs {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(i64::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
impl JsonSchema for EpochMs {
    fn schema_name() -> Cow<'static, str> {
        "EpochMs".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"integer","minimum":-8640000000000000_i64,"maximum":8640000000000000_i64})
    }
}

pub fn sum_checked(values: impl IntoIterator<Item = i128>) -> Result<i128, ErrorCode> {
    values.into_iter().try_fold(0_i128, |sum, v| {
        sum.checked_add(v).ok_or(ErrorCode::NumericOverflow)
    })
}
