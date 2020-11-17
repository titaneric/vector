use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct StripWhitespace;

impl Function for StripWhitespace {
    fn identifier(&self) -> &'static str {
        "strip_whitespace"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::String(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;

        Ok(Box::new(StripWhitespaceFn { value }))
    }
}

#[derive(Debug, Clone)]
struct StripWhitespaceFn {
    value: Box<dyn Expression>,
}

impl StripWhitespaceFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for StripWhitespaceFn {
    fn execute(
        &self,
        state: &mut state::Program,
        object: &mut dyn Object,
    ) -> Result<Option<Value>> {
        let value = required!(state, object, self.value, Value::String(b) => String::from_utf8_lossy(&b).into_owned());

        Ok(Some(value.trim().into()))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::String)
            .with_constraint(value::Kind::String)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    remap::test_type_def![
        value_string {
            expr: |_| StripWhitespaceFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: value::Kind::String, ..Default::default() },
        }

        fallible_expression {
            expr: |_| StripWhitespaceFn { value: Literal::from(10).boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::String, ..Default::default() },
        }
    ];

    #[test]
    fn strip_whitespace() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": ""],
                Ok(Some("".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "     "],
                Ok(Some("".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "hi there"],
                Ok(Some("hi there".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "           hi there        "],
                Ok(Some("hi there".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": " \u{3000}\u{205F}\u{202F}\u{A0}\u{9} ❤❤ hi there ❤❤  \u{9}\u{A0}\u{202F}\u{205F}\u{3000} "],
                Ok(Some("❤❤ hi there ❤❤".into())),
                StripWhitespaceFn::new(Box::new(Path::from("foo"))),
            ),
        ];

        let mut state = state::Program::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
