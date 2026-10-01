use rusqlite::{
    Connection, Result,
    functions::{Aggregate, Context, FunctionFlags},
};
use token_pulse_core::{error::ErrorCode, numeric::DecimalInt};

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
        "sum_token_decimal",
        1,
        flags,
        ExactSum { money: false },
    )?;
    connection.create_aggregate_function("sum_money_atoms", 1, flags, ExactSum { money: true })?;
    Ok(())
}
