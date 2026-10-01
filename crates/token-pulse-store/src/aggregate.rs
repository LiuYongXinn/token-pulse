use rusqlite::{
    Connection, Result,
    functions::{Aggregate, Context, FunctionFlags},
};
use token_pulse_core::{
    domain::UsageVector, error::ErrorCode, numeric::DecimalInt, protocol::TokenMeasure,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct TokenSums {
    pub total: DecimalInt,
    pub measures: [TokenMeasure; 5],
}

#[derive(Default)]
pub(crate) struct VectorAccumulator {
    total: i128,
    values: [Option<i128>; 5],
    covered: [i128; 5],
    known: [u64; 5],
    events: u64,
}
struct VectorSum;
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
            output_total: ctx.get(2)?,
            reasoning_output: ctx.get(3)?,
            reported_total: Some(ctx.get(4)?),
        };
        acc.add(vector).map_err(function_error)
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
            vector.noncached_input()?,
            vector.output_total,
            vector.reasoning_output,
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
        let mut measures = Vec::with_capacity(5);
        for i in 0..5 {
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
    connection.create_aggregate_function("sum_usage_vector", 5, flags, VectorSum)?;
    connection.create_scalar_function("usage_model_key", 2, flags, |ctx| {
        let provider: Option<String> = ctx.get(0)?;
        let model: Option<String> = ctx.get(1)?;
        Ok(token_pulse_core::query::model_key(
            provider.as_deref(),
            model.as_deref(),
        ))
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
