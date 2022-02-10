use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Append;

impl Function for Append {
    fn identifier(&self) -> &'static str {
        "append"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "items",
                kind: kind::ARRAY,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "append array",
            source: r#"append([0, 1], [2, 3])"#,
            result: Ok("[0, 1, 2, 3]"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let items = arguments.required("items");

        Ok(Box::new(AppendFn { value, items }))
    }
}

#[derive(Debug, Clone)]
struct AppendFn {
    value: Box<dyn Expression>,
    items: Box<dyn Expression>,
}

impl Expression for AppendFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let mut value = self.value.resolve(ctx)?.try_array()?;
        let mut items = self.items.resolve(ctx)?.try_array()?;

        value.append(&mut items);

        Ok(value.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .merge_append(self.items.type_def(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_common::btreemap;

    test_function![
        append => Append;

        both_arrays_empty {
            args: func_args![value: value!([]), items: value!([])],
            want: Ok(value!([])),
            tdef: TypeDef::array(Collection::empty()),
        }

        one_array_empty {
            args: func_args![value: value!([]), items: value!([1, 2, 3])],
            want: Ok(value!([1, 2, 3])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::integer(),
                Index::from(2) => Kind::integer(),
            }),
        }

        neither_array_empty {
            args: func_args![value: value!([1, 2, 3]), items: value!([4, 5, 6])],
            want: Ok(value!([1, 2, 3, 4, 5, 6])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::integer(),
                Index::from(2) => Kind::integer(),
                Index::from(3) => Kind::integer(),
                Index::from(4) => Kind::integer(),
                Index::from(5) => Kind::integer(),
            }),
        }

        mixed_array_types {
            args: func_args![value: value!([1, 2, 3]), items: value!([true, 5.0, "bar"])],
            want: Ok(value!([1, 2, 3, true, 5.0, "bar"])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::integer(),
                Index::from(2) => Kind::integer(),
                Index::from(3) => Kind::boolean(),
                Index::from(4) => Kind::float(),
                Index::from(5) => Kind::bytes(),
            }),
        }
    ];
}
