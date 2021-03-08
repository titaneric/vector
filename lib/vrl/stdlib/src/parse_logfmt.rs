use crate::parse_key_value::ParseKeyValueFn;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseLogFmt;

impl Function for ParseLogFmt {
    fn identifier(&self) -> &'static str {
        "parse_logfmt"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "simple log",
            source: r#"parse_logfmt!("zork=zook zonk=nork")"#,
            result: Ok(r#"{"zork": "zook", "zonk": "nork"}"#),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        // The parse_logfmt function is just an alias for `parse_key_value` with the following
        // parameters for the delimiters.
        let key_value_delimiter = expr!("=");
        let field_delimiter = expr!(" ");

        Ok(Box::new(ParseKeyValueFn {
            value,
            key_value_delimiter,
            field_delimiter,
        }))
    }
}
