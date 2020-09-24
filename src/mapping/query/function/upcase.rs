use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct UpcaseFn {
    query: Box<dyn Function>,
}

impl UpcaseFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for UpcaseFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx)? {
            Value::Bytes(bytes) => Ok(Value::Bytes(
                String::from_utf8_lossy(&bytes).to_uppercase().into(),
            )),
            v => unexpected_type!(v),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for UpcaseFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn upcase() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                UpcaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("foo 2 bar"));
                    event
                },
                Ok(Value::from("FOO 2 BAR")),
                UpcaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'integer'")]
    fn invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Integer(20));

        let _ = UpcaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))).execute(&event);
    }
}
