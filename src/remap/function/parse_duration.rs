use lazy_static::lazy_static;
use regex::Regex;
use remap::prelude::*;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::collections::HashMap;
use std::str::FromStr;

lazy_static! {
    static ref RE: Regex = Regex::new(
        r"(?ix)                        # i: case-insensitive, x: ignore whitespace + comments
            \A
            (?P<value>[0-9]*\.?[0-9]+) # value: integer or float
            \s?                        # optional space between value and unit
            (?P<unit>[a-z]{1,2})       # unit: one or two letters
            \z"
    )
    .unwrap();
    static ref UNITS: HashMap<String, Decimal> = vec![
        ("ns", Decimal::new(1, 9)),
        ("us", Decimal::new(1, 6)),
        ("µs", Decimal::new(1, 6)),
        ("ms", Decimal::new(1, 3)),
        ("cs", Decimal::new(1, 2)),
        ("ds", Decimal::new(1, 1)),
        ("s", Decimal::new(1, 0)),
        ("m", Decimal::new(60, 0)),
        ("h", Decimal::new(3_600, 0)),
        ("d", Decimal::new(86_400, 0)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
}

#[derive(Clone, Copy, Debug)]
pub struct ParseDuration;

impl Function for ParseDuration {
    fn identifier(&self) -> &'static str {
        "parse_duration"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "output",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let output = arguments.required_expr("output")?;

        Ok(Box::new(ParseDurationFn { value, output }))
    }
}

#[derive(Debug)]
struct ParseDurationFn {
    value: Box<dyn Expression>,
    output: Box<dyn Expression>,
}

impl ParseDurationFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, output: &str) -> Self {
        let output = Box::new(Literal::from(output));

        Self { value, output }
    }
}

impl Expression for ParseDurationFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let value = {
            let bytes = required!(state, object, self.value, Value::String(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let conversion_factor = {
            let bytes = required!(state, object, self.output, Value::String(v) => v);
            let output = String::from_utf8_lossy(&bytes).into_owned();

            UNITS
                .get(&output)
                .ok_or(format!("unknown output format: '{}'", output))?
        };

        let captures = RE
            .captures(&value)
            .ok_or(format!("unable to parse duration: '{}'", value))?;

        let value = Decimal::from_str(&captures["value"])
            .map_err(|error| format!("unable to parse number: {}", error))?;

        let unit = UNITS
            .get(&captures["unit"])
            .ok_or(format!("unknown duration unit: '{}'", &captures["unit"]))?;

        let number = value * unit / conversion_factor;
        let number = number
            .to_f64()
            .ok_or(format!("unable to format duration: '{}'", number))?;

        Ok(Some(number.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn parse_duration() {
        let cases = vec![
            (
                map![],
                Ok(Some(0.5.into())),
                ParseDurationFn::new(Box::new(Literal::from("30s")), "m"),
            ),
            (
                map![],
                Ok(Some(1.2.into())),
                ParseDurationFn::new(Box::new(Literal::from("1200ms")), "s"),
            ),
            (
                map![],
                Ok(Some(100.0.into())),
                ParseDurationFn::new(Box::new(Literal::from("100ms")), "ms"),
            ),
            (
                map![],
                Ok(Some(1.005.into())),
                ParseDurationFn::new(Box::new(Literal::from("1005ms")), "s"),
            ),
            (
                map![],
                Ok(Some(0.0001.into())),
                ParseDurationFn::new(Box::new(Literal::from("100ns")), "ms"),
            ),
            (
                map![],
                Ok(Some(86400.0.into())),
                ParseDurationFn::new(Box::new(Literal::from("1d")), "s"),
            ),
            (
                map![],
                Ok(Some(1000000000.0.into())),
                ParseDurationFn::new(Box::new(Literal::from("1 s")), "ns"),
            ),
            (
                map![],
                Err("path error: missing path: foo".into()),
                ParseDurationFn::new(Box::new(Path::from("foo")), "s"),
            ),
            (
                map![],
                Err("function call error: unable to parse duration: 'foo'".into()),
                ParseDurationFn::new(Box::new(Literal::from("foo")), "µs"),
            ),
            (
                map![],
                Err("function call error: unable to parse duration: '1'".into()),
                ParseDurationFn::new(Box::new(Literal::from("1")), "ns"),
            ),
            (
                map![],
                Err("function call error: unknown duration unit: 'w'".into()),
                ParseDurationFn::new(Box::new(Literal::from("1w")), "ns"),
            ),
            (
                map![],
                Err("function call error: unknown output format: 'w'".into()),
                ParseDurationFn::new(Box::new(Literal::from("1s")), "w"),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
