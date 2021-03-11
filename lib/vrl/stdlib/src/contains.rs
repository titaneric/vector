use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Contains;

impl Function for Contains {
    fn identifier(&self) -> &'static str {
        "contains"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "substring",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "case_sensitive",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let substring = arguments.required("substring");
        let case_sensitive = arguments.optional("case_sensitive");

        Ok(Box::new(ContainsFn {
            value,
            substring,
            case_sensitive,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "case sensitive",
                source: r#"contains("banana", "ana")"#,
                result: Ok(r#"true"#),
            },
            Example {
                title: "case insensitive",
                source: r#"contains("banana", "AnA", case_sensitive: false)"#,
                result: Ok(r#"true"#),
            },
        ]
    }
}

#[derive(Clone, Debug)]
struct ContainsFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Option<Box<dyn Expression>>,
}

impl Expression for ContainsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let case_sensitive = match &self.case_sensitive {
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
            None => false,
        };

        let substring = {
            let bytes = self.substring.resolve(ctx)?.try_bytes()?;
            let string = String::from_utf8_lossy(&bytes);

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        let value = {
            let value = self.value.resolve(ctx)?;
            let string = value.try_bytes_utf8_lossy()?;

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        Ok(value.contains(&substring).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().boolean().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        contains => Contains;

        no {
            args: func_args![value: value!("foo"),
                             substring: value!("bar")],
            want: Ok(value!(false)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        yes {
            args: func_args![value: value!("foobar"),
                             substring: value!("foo")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        entirely {
            args: func_args![value: value!("foo"),
                             substring: value!("foo")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        middle {
            args: func_args![value: value!("foobar"),
                             substring: value!("oba")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        start {
            args: func_args![value: value!("foobar"),
                             substring: value!("foo")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        end {
            args: func_args![value: value!("foobar"),
                             substring: value!("bar")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        case_sensitive_yes {
            args: func_args![value: value!("fooBAR"),
                             substring: value!("BAR"),
                             case_sensitive: true
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

         case_sensitive_yes_lowercase {
            args: func_args![value: value!("fooBAR"),
                             substring: value!("bar"),
                             case_sensitive: true
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        case_sensitive_no_uppercase {
            args: func_args![value: value!("foobar"),
                             substring: value!("BAR"),
                             case_sensitive: true
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        case_insensitive_yes_uppercase {
            args: func_args![value: value!("foobar"),
                             substring: value!("BAR"),
                             case_sensitive: false
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }
    ];
}
