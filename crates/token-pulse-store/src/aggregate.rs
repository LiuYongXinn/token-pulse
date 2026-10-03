use rusqlite::{
    Connection, Result,
    functions::{Aggregate, Context, FunctionFlags},
};
use token_pulse_core::{
    domain::UsageVector, error::ErrorCode, numeric::DecimalInt, protocol::TokenMeasure,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TokenSums {
    pub total: DecimalInt,
    pub measures: [TokenMeasure; 6],
}

#[derive(Default)]
pub(crate) struct VectorAccumulator {
    total: i128,
    values: [Option<i128>; 6],
    covered: [i128; 6],
    known: [u64; 6],
    events: u64,
}
struct VectorSum {
    versioned: bool,
    cache_write: bool,
}
#[derive(Default)]
pub(crate) struct ProjectionAccumulator {
    total: i128,
    values: [Option<i128>; 6],
    covered: [i128; 6],
    incomplete: [bool; 6],
    events: i128,
}
impl ProjectionAccumulator {
    pub fn add(&mut self, sums: &TokenSums, events: i128) -> std::result::Result<(), ErrorCode> {
        if events < 0
            || (events == 0
                && (sums.total.value() != 0
                    || sums.measures.iter().any(|m| {
                        m.value.is_some() || m.complete || m.covered_total_tokens.value() != 0
                    })))
        {
            return Err(ErrorCode::InvalidUsage);
        }
        if events == 0 {
            return Ok(());
        }
        self.total = self
            .total
            .checked_add(sums.total.value())
            .ok_or(ErrorCode::NumericOverflow)?;
        self.events = self
            .events
            .checked_add(events)
            .ok_or(ErrorCode::NumericOverflow)?;
        for (i, m) in sums.measures.iter().enumerate() {
            if m.covered_total_tokens.value() > sums.total.value()
                || (m.value.is_none() && (m.complete || m.covered_total_tokens.value() != 0))
                || (m.complete && m.covered_total_tokens.value() != sums.total.value())
            {
                return Err(ErrorCode::InvalidUsage);
            }
            self.covered[i] = self.covered[i]
                .checked_add(m.covered_total_tokens.value())
                .ok_or(ErrorCode::NumericOverflow)?;
            self.incomplete[i] |= !m.complete;
            if let Some(value) = &m.value {
                self.values[i] = Some(
                    self.values[i]
                        .unwrap_or(0)
                        .checked_add(value.value())
                        .ok_or(ErrorCode::NumericOverflow)?,
                );
            }
        }
        Ok(())
    }
    pub fn finish(&self) -> std::result::Result<TokenSums, ErrorCode> {
        let mut measures = Vec::with_capacity(6);
        for i in 0..6 {
            measures.push(TokenMeasure {
                value: self.values[i]
                    .map(DecimalInt::from_nonnegative)
                    .transpose()?,
                covered_total_tokens: DecimalInt::from_nonnegative(self.covered[i])?,
                complete: self.events > 0 && !self.incomplete[i],
            });
        }
        Ok(TokenSums {
            total: DecimalInt::from_nonnegative(self.total)?,
            measures: measures.try_into().map_err(|_| ErrorCode::DbCorrupt)?,
        })
    }
}
struct ProjectionSum;
impl Aggregate<ProjectionAccumulator, String> for ProjectionSum {
    fn init(&self, _: &mut Context<'_>) -> Result<ProjectionAccumulator> {
        Ok(ProjectionAccumulator::default())
    }
    fn step(&self, ctx: &mut Context<'_>, acc: &mut ProjectionAccumulator) -> Result<()> {
        let encoded: String = ctx.get(0)?;
        let sums: TokenSums =
            serde_json::from_str(&encoded).map_err(|_| function_error(ErrorCode::DbCorrupt))?;
        let events: i64 = ctx.get(1)?;
        acc.add(&sums, events.into()).map_err(function_error)
    }
    fn finalize(&self, _: &mut Context<'_>, acc: Option<ProjectionAccumulator>) -> Result<String> {
        let sums = acc.unwrap_or_default().finish().map_err(function_error)?;
        serde_json::to_string(&sums).map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))
    }
}
fn function_error(error: ErrorCode) -> rusqlite::Error {
    rusqlite::Error::UserFunctionError(Box::new(error))
}
impl Aggregate<VectorAccumulator, String> for VectorSum {
    fn init(&self, _: &mut Context<'_>) -> Result<VectorAccumulator> {
        Ok(VectorAccumulator::default())
    }
    fn step(&self, ctx: &mut Context<'_>, acc: &mut VectorAccumulator) -> Result<()> {
        let vector = UsageVector {
            input_total: ctx.get(0)?,
            cached_input: ctx.get(1)?,
            cache_write_input: if self.cache_write {
                ctx.get(if self.versioned { 6 } else { 5 })?
            } else {
                None
            },
            output_total: ctx.get(2)?,
            reasoning_output: ctx.get(3)?,
            reported_total: Some(ctx.get(4)?),
        };
        if self.versioned {
            let version: String = ctx.get(5)?;
            acc.add_published(vector, &version).map_err(function_error)
        } else {
            acc.add(vector).map_err(function_error)
        }
    }
    fn finalize(&self, _: &mut Context<'_>, acc: Option<VectorAccumulator>) -> Result<String> {
        let sums = acc.unwrap_or_default().finish().map_err(function_error)?;
        serde_json::to_string(&sums).map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))
    }
}

impl VectorAccumulator {
    pub fn event_count(&self) -> u64 {
        self.events
    }
    pub fn add(&mut self, vector: UsageVector) -> std::result::Result<(), ErrorCode> {
        let total = i128::from(vector.validated_total()?.ok_or(ErrorCode::InvalidUsage)?);
        self.add_validated(vector, total)
    }
    pub fn add_published(
        &mut self,
        vector: UsageVector,
        version: &str,
    ) -> std::result::Result<(), ErrorCode> {
        let total = i128::from(
            vector
                .published_total(version)?
                .ok_or(ErrorCode::InvalidUsage)?,
        );
        self.add_validated(vector, total)
    }
    fn add_validated(
        &mut self,
        vector: UsageVector,
        total: i128,
    ) -> std::result::Result<(), ErrorCode> {
        self.total = self
            .total
            .checked_add(total)
            .ok_or(ErrorCode::NumericOverflow)?;
        self.events = self
            .events
            .checked_add(1)
            .ok_or(ErrorCode::NumericOverflow)?;
        let values = [
            vector.input_total,
            vector.cached_input,
            match (vector.input_total, vector.cached_input) {
                (Some(input), Some(cached)) => Some(input - cached),
                _ => None,
            },
            vector.output_total,
            vector.reasoning_output,
            vector.cache_write_input,
        ];
        for (i, value) in values.into_iter().enumerate() {
            if let Some(value) = value {
                self.values[i] = Some(
                    self.values[i]
                        .unwrap_or(0)
                        .checked_add(value.into())
                        .ok_or(ErrorCode::NumericOverflow)?,
                );
                self.covered[i] = self.covered[i]
                    .checked_add(total)
                    .ok_or(ErrorCode::NumericOverflow)?;
                self.known[i] = self.known[i]
                    .checked_add(1)
                    .ok_or(ErrorCode::NumericOverflow)?;
            }
        }
        Ok(())
    }
    pub fn finish(&self) -> std::result::Result<TokenSums, ErrorCode> {
        let convert = DecimalInt::from_nonnegative;
        let mut measures = Vec::with_capacity(6);
        for i in 0..6 {
            measures.push(TokenMeasure {
                value: self.values[i].map(convert).transpose()?,
                covered_total_tokens: convert(self.covered[i])?,
                complete: self.events > 0 && self.known[i] == self.events,
            });
        }
        Ok(TokenSums {
            total: convert(self.total)?,
            measures: measures.try_into().map_err(|_| ErrorCode::DbCorrupt)?,
        })
    }
}

struct ExactSum {
    money: bool,
}
impl Aggregate<Option<i128>, Option<String>> for ExactSum {
    fn init(&self, _: &mut Context<'_>) -> Result<Option<i128>> {
        Ok(None)
    }
    fn step(&self, ctx: &mut Context<'_>, acc: &mut Option<i128>) -> Result<()> {
        let value = if self.money {
            ctx.get::<Option<String>>(0)?
                .map(|s| DecimalInt::parse(&s).map(|d| d.value()))
                .transpose()
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?
        } else {
            ctx.get::<Option<i64>>(0)?.map(i128::from)
        };
        if let Some(v) = value {
            if v < 0 {
                return Err(rusqlite::Error::UserFunctionError(Box::new(
                    ErrorCode::InvalidUsage,
                )));
            }
            *acc = Some(acc.unwrap_or(0).checked_add(v).ok_or_else(|| {
                rusqlite::Error::UserFunctionError(Box::new(ErrorCode::NumericOverflow))
            })?);
        }
        Ok(())
    }
    fn finalize(&self, _: &mut Context<'_>, acc: Option<Option<i128>>) -> Result<Option<String>> {
        Ok(acc.flatten().map(|n| n.to_string()))
    }
}
pub fn register(connection: &Connection) -> Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    connection.create_aggregate_function(
        "sum_usage_vector",
        5,
        flags,
        VectorSum {
            versioned: false,
            cache_write: false,
        },
    )?;
    connection.create_aggregate_function(
        "sum_published_usage_vector",
        6,
        flags,
        VectorSum {
            versioned: true,
            cache_write: false,
        },
    )?;
    connection.create_aggregate_function(
        "sum_usage_vector",
        6,
        flags,
        VectorSum {
            versioned: false,
            cache_write: true,
        },
    )?;
    connection.create_aggregate_function(
        "sum_published_usage_vector",
        7,
        flags,
        VectorSum {
            versioned: true,
            cache_write: true,
        },
    )?;
    connection.create_aggregate_function("sum_usage_projection", 2, flags, ProjectionSum)?;
    connection.create_scalar_function("usage_model_key", 2, flags, |ctx| {
        let provider: Option<String> = ctx.get(0)?;
        let model: Option<String> = ctx.get(1)?;
        Ok(token_pulse_core::query::model_key(
            provider.as_deref(),
            model.as_deref(),
        ))
    })?;
    connection.create_scalar_function("usage_search_contains", 2, flags, |ctx| {
        let text: String = ctx.get(0)?;
        let search: String = ctx.get(1)?;
        Ok(text.to_lowercase().contains(&search.to_lowercase()))
    })?;
    connection.create_scalar_function("usage_vector_total", 1, flags, |ctx| {
        let json: Option<String> = ctx.get(0)?;
        let Some(json) = json else {
            return Ok(None::<i64>);
        };
        let vector: UsageVector = serde_json::from_str(&json)
            .map_err(|_| rusqlite::Error::UserFunctionError(Box::new(ErrorCode::DbCorrupt)))?;
        // Pending observations can have an invalid or incomplete vector. Only
        // their provable total contributes to an unattributed amount.
        Ok(vector.validated_total().ok().flatten())
    })?;
    connection.create_aggregate_function(
        "sum_token_decimal",
        1,
        flags,
        ExactSum { money: false },
    )?;
    connection.create_aggregate_function("sum_money_atoms", 1, flags, ExactSum { money: true })?;
    Ok(())
}
