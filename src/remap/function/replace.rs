use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Replace;

impl Function for Replace {
    fn identifier(&self) -> &'static str {
        "replace"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "with",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "count",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let pattern = arguments.required_expr_or_regex("pattern")?;
        let with = arguments.required_expr("with")?;
        let count = arguments.optional_expr("count")?;

        Ok(Box::new(ReplaceFn {
            value,
            pattern,
            with,
            count,
        }))
    }
}

#[derive(Debug, Clone)]
struct ReplaceFn {
    value: Box<dyn Expression>,
    pattern: Argument,
    with: Box<dyn Expression>,
    count: Option<Box<dyn Expression>>,
}

impl ReplaceFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, pattern: Argument, with: &str, count: Option<i32>) -> Self {
        let with = Box::new(Literal::from(Value::from(with)));
        let count = count.map(Literal::from).map(|v| Box::new(v) as _);

        ReplaceFn {
            value,
            pattern,
            with,
            count,
        }
    }
}

impl Expression for ReplaceFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value_bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&value_bytes);

        let with_bytes = self.with.execute(state, object)?.try_bytes()?;
        let with = String::from_utf8_lossy(&with_bytes);

        let count = match &self.count {
            Some(expr) => expr.execute(state, object)?.try_integer()?,
            None => -1,
        };

        match &self.pattern {
            Argument::Expression(expr) => {
                let bytes = expr.execute(state, object)?.try_bytes()?;
                let pattern = String::from_utf8_lossy(&bytes);
                let replaced = match count {
                    i if i > 0 => value.replacen(pattern.as_ref(), &with, i as usize),
                    i if i < 0 => value.replace(pattern.as_ref(), &with),
                    _ => value.into_owned(),
                };

                Ok(replaced.into())
            }
            Argument::Regex(regex) => {
                let replaced = match count {
                    i if i > 0 => regex
                        .replacen(&value, i as usize, with.as_ref())
                        .as_bytes()
                        .into(),
                    i if i < 0 => regex.replace_all(&value, with.as_ref()).as_bytes().into(),
                    _ => value.into(),
                };

                Ok(replaced)
            }
            Argument::Array(_) => unreachable!(),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let with_def = self.with.type_def(state).fallible_unless(Kind::Bytes);

        let count_def = self
            .count
            .as_ref()
            .map(|count| count.type_def(state).fallible_unless(Kind::Integer));

        let pattern_def = match &self.pattern {
            Argument::Expression(expr) => Some(expr.type_def(state).fallible_unless(Kind::Bytes)),
            Argument::Regex(_) => None, // regex is a concrete infallible type
            Argument::Array(_) => unreachable!(),
        };

        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .merge(with_def)
            .merge_optional(pattern_def)
            .merge_optional(count_def)
            .with_constraint(Kind::Bytes)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::map;

    remap::test_type_def![
        infallible {
            expr: |_| ReplaceFn {
                value: Literal::from("foo").boxed(),
                pattern: regex::Regex::new("foo").unwrap().into(),
                with: Literal::from("foo").boxed(),
                count: None,
            },
            def: TypeDef {
                kind: value::Kind::Bytes,
                ..Default::default()
            },
        }

        value_fallible {
            expr: |_| ReplaceFn {
                value: Literal::from(10).boxed(),
                pattern: regex::Regex::new("foo").unwrap().into(),
                with: Literal::from("foo").boxed(),
                count: None,
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Bytes,
            },
        }

        pattern_expression_infallible {
            expr: |_| ReplaceFn {
                value: Literal::from("foo").boxed(),
                pattern: Literal::from("foo").into(),
                with: Literal::from("foo").boxed(),
                count: None,
            },
            def: TypeDef {
                kind: value::Kind::Bytes,
                ..Default::default()
            },
        }

        pattern_expression_fallible {
            expr: |_| ReplaceFn {
                value: Literal::from("foo").boxed(),
                pattern: Literal::from(10).into(),
                with: Literal::from("foo").boxed(),
                count: None,
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Bytes,
            },
        }

        with_fallible {
            expr: |_| ReplaceFn {
                value: Literal::from("foo").boxed(),
                pattern: regex::Regex::new("foo").unwrap().into(),
                with: Literal::from(10).boxed(),
                count: None,
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Bytes,
            },
        }

        count_infallible {
            expr: |_| ReplaceFn {
                value: Literal::from("foo").boxed(),
                pattern: regex::Regex::new("foo").unwrap().into(),
                with: Literal::from("foo").boxed(),
                count: Some(Literal::from(10).boxed()),
            },
            def: TypeDef {
                kind: value::Kind::Bytes,
                ..Default::default()
            },
        }

        count_fallible {
            expr: |_| ReplaceFn {
                value: Literal::from("foo").boxed(),
                pattern: regex::Regex::new("foo").unwrap().into(),
                with: Literal::from("foo").boxed(),
                count: Some(Literal::from("foo").boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Bytes,
            },
        }
    ];

    #[test]
    fn check_replace_string() {
        let cases = vec![
            (
                map![],
                Ok("I like opples ond bononos".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    Literal::from("a").into(),
                    "o",
                    None,
                ),
            ),
            (
                map![],
                Ok("I like opples ond bononos".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    Literal::from("a").into(),
                    "o",
                    Some(-1),
                ),
            ),
            (
                map![],
                Ok("I like apples and bananas".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    Literal::from("a").into(),
                    "o",
                    Some(0),
                ),
            ),
            (
                map![],
                Ok("I like opples and bananas".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    Literal::from("a").into(),
                    "o",
                    Some(1),
                ),
            ),
            (
                map![],
                Ok("I like opples ond bananas".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    Literal::from("a").into(),
                    "o",
                    Some(2),
                ),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }

    #[test]
    fn check_replace_regex() {
        let cases = vec![
            (
                map![],
                Ok("I like opples ond bononos".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    regex::Regex::new("a").unwrap().into(),
                    "o",
                    None,
                ),
            ),
            (
                map![],
                Ok("I like opples ond bononos".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    regex::Regex::new("a").unwrap().into(),
                    "o",
                    Some(-1),
                ),
            ),
            (
                map![],
                Ok("I like apples and bananas".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    regex::Regex::new("a").unwrap().into(),
                    "o",
                    Some(0),
                ),
            ),
            (
                map![],
                Ok("I like opples and bananas".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    regex::Regex::new("a").unwrap().into(),
                    "o",
                    Some(1),
                ),
            ),
            (
                map![],
                Ok("I like opples ond bananas".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    regex::Regex::new("a").unwrap().into(),
                    "o",
                    Some(2),
                ),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }

    #[test]
    fn check_replace_other() {
        let cases = vec![
            (
                map![],
                Ok("I like biscuits and bananas".into()),
                ReplaceFn::new(
                    Literal::from("I like apples and bananas").boxed(),
                    Literal::from("apples").into(),
                    "biscuits",
                    None,
                ),
            ),
            (
                map!["foo": "I like apples and bananas"],
                Ok("I like opples and bananas".into()),
                ReplaceFn::new(
                    Box::new(Path::from("foo")),
                    regex::Regex::new("a").unwrap().into(),
                    "o",
                    Some(1),
                ),
            ),
            (
                map!["foo": "I like [apples] and bananas"],
                Ok("I like biscuits and bananas".into()),
                ReplaceFn::new(
                    Box::new(Path::from("foo")),
                    regex::Regex::new("\\[apples\\]").unwrap().into(),
                    "biscuits",
                    None,
                ),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
