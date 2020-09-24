use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct ToBooleanFn {
    query: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ToBooleanFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { query, default }
    }
}

impl Function for ToBooleanFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx) {
            Ok(v) => match v {
                Value::Boolean(_) => Ok(v),
                Value::Float(f) => Ok(Value::Boolean(f != 0.0)),
                Value::Integer(i) => Ok(Value::Boolean(i != 0)),
                Value::Bytes(_) => Conversion::Boolean.convert(v).map_err(|e| e.to_string()),
                _ => unexpected_type!(v),
            },
            Err(err) => Err(err),
        }
        .or_else(|err| match &self.default {
            Some(v) => v.execute(ctx),
            None => Err(err),
        })
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Float(_) | Value::Bytes(_) | Value::Boolean(_)),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Float(_) | Value::Bytes(_) | Value::Boolean(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ToBooleanFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let default = arguments.optional("default");

        Ok(Self { query, default })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn to_bool() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ToBooleanFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(true)),
                ToBooleanFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("true"));
                    event
                },
                Ok(Value::Boolean(true)),
                ToBooleanFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(20));
                    event
                },
                Ok(Value::Boolean(true)),
                ToBooleanFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
