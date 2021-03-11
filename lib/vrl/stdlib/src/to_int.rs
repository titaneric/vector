use shared::conversion::Conversion;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToInt;

impl Function for ToInt {
    fn identifier(&self) -> &'static str {
        "to_int"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "integer",
                source: "to_int(5)",
                result: Ok("5"),
            },
            Example {
                title: "float",
                source: "to_int(5.6)",
                result: Ok("6"),
            },
            Example {
                title: "true",
                source: "to_int(true)",
                result: Ok("1"),
            },
            Example {
                title: "false",
                source: "to_int(false)",
                result: Ok("0"),
            },
            Example {
                title: "null",
                source: "to_int(null)",
                result: Ok("0"),
            },
            Example {
                title: "timestamp",
                source: "to_int(t'2020-01-01T00:00:00Z')",
                result: Ok("1577836800"),
            },
            Example {
                title: "valid string",
                source: "to_int!(s'5')",
                result: Ok("5"),
            },
            Example {
                title: "invalid string",
                source: "to_int!(s'foobar')",
                result: Err(
                    r#"function call error for "to_int" at (0:18): Invalid integer "foobar": invalid digit found in string"#,
                ),
            },
            Example {
                title: "array",
                source: "to_int!([])",
                result: Err(
                    r#"function call error for "to_int" at (0:11): unable to coerce "array" into "integer""#,
                ),
            },
            Example {
                title: "object",
                source: "to_int!({})",
                result: Err(
                    r#"function call error for "to_int" at (0:11): unable to coerce "object" into "integer""#,
                ),
            },
            Example {
                title: "regex",
                source: "to_int!(r'foo')",
                result: Err(
                    r#"function call error for "to_int" at (0:15): unable to coerce "regex" into "integer""#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToIntFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToIntFn {
    value: Box<dyn Expression>,
}

impl Expression for ToIntFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Value::*;

        let value = self.value.resolve(ctx)?;

        match value {
            Integer(_) => Ok(value),
            Float(v) => Ok(Integer(v.into_inner().round() as i64)),
            Boolean(v) => Ok(Integer(if v { 1 } else { 0 })),
            Null => Ok(0.into()),
            Bytes(v) => Conversion::Integer
                .convert(v)
                .map_err(|e| e.to_string().into()),
            Timestamp(v) => Ok(v.timestamp().into()),
            v => Err(format!(r#"unable to coerce {} into "integer""#, v.kind()).into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .with_fallibility(
                self.value
                    .type_def(state)
                    .has_kind(Kind::Bytes | Kind::Array | Kind::Object | Kind::Regex),
            )
            .integer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    test_function![
        to_int => ToInt;

        string {
             args: func_args![value: "20"],
             want: Ok(20),
             tdef: TypeDef::new().fallible().integer(),
        }

        float {
             args: func_args![value: 20.5],
             want: Ok(21),
             tdef: TypeDef::new().infallible().integer(),
        }

        timezone {
             args: func_args![value: DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                            .unwrap()
                            .with_timezone(&Utc)],
             want: Ok(1571227200),
             tdef: TypeDef::new().infallible().integer(),
         }
    ];
}
