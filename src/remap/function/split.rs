use remap::prelude::*;
use std::convert::{TryFrom, TryInto};

#[derive(Clone, Copy, Debug)]
pub struct Split;

impl Function for Split {
    fn identifier(&self) -> &'static str {
        "split"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "limit",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let pattern = arguments.required("pattern")?;
        let limit = arguments.optional_expr("limit")?;

        Ok(Box::new(SplitFn {
            value,
            pattern,
            limit,
        }))
    }
}

#[derive(Debug)]
pub(crate) struct SplitFn {
    value: Box<dyn Expression>,
    pattern: Argument,
    limit: Option<Box<dyn Expression>>,
}

impl Expression for SplitFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let string: String = self
            .value
            .execute(state, object)?
            .ok_or_else(|| Error::from("argument missing"))?
            .try_into()?;

        let limit: usize = self
            .limit
            .as_ref()
            .and_then(|expr| expr.execute(state, object).transpose())
            .transpose()?
            .map(i64::try_from)
            .transpose()?
            .and_then(|i| usize::try_from(i).ok())
            .unwrap_or(usize::MAX);

        let value = match &self.pattern {
            Argument::Regex(pattern) => pattern
                .splitn(&string, limit as usize)
                .collect::<Vec<_>>()
                .into(),
            Argument::Expression(expr) => {
                let pattern: String = expr
                    .execute(state, object)?
                    .ok_or_else(|| Error::from("argument missing"))?
                    .try_into()?;

                string.splitn(limit, &pattern).collect::<Vec<_>>().into()
            }
        };

        Ok(Some(value))
    }
}
