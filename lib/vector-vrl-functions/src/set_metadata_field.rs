use crate::{get_metadata_key, MetadataKey};
use ::value::Value;
use vrl::prelude::*;
use vrl::query::Target as QueryTarget;
use vrl::state::{ExternalEnv, LocalEnv};

fn set_metadata_field(
    ctx: &mut Context,
    key: &MetadataKey,
    value: Value,
) -> std::result::Result<Value, ExpressionError> {
    Ok(match key {
        MetadataKey::Legacy(key) => {
            let str_value = value.as_str().expect("must be a string");
            ctx.target_mut().insert_secret(key, str_value.as_ref());
            Value::Null
        }
        MetadataKey::Query(query) => {
            ctx.target_mut().set_metadata(query.path(), value)?;
            Value::Null
        }
    })
}

#[derive(Clone, Copy, Debug)]
pub struct SetMetadataField;

impl Function for SetMetadataField {
    fn identifier(&self) -> &'static str {
        "set_metadata_field"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "key",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Sets the datadog api key",
            source: r#"set_metadata_field("datadog_api_key", "abc123")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        (local, external): (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let key = get_metadata_key(&mut arguments)?;
        let value = arguments.required_expr("value");

        if let MetadataKey::Query(query) = &key {
            if external.is_read_only_metadata_path(query.path()) {
                return Err(vrl::function::Error::ReadOnlyMutation {
                    context: format!("{} is read-only, and cannot be modified", query),
                }
                .into());
            }
        }

        // for backwards compatibility, make sure value is a string when using legacy.
        if matches!(key, MetadataKey::Legacy(_)) && !value.type_def((local, external)).is_bytes() {
            return Err(vrl::function::Error::UnexpectedExpression {
                keyword: "value",
                expected: "string",
                expr: value,
            }
            .into());
        }

        Ok(Box::new(SetMetadataFieldFn {
            key,
            value: Box::new(value),
        }))
    }
}

#[derive(Debug, Clone)]
struct SetMetadataFieldFn {
    key: MetadataKey,
    value: Box<dyn Expression>,
}

impl Expression for SetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        set_metadata_field(ctx, &self.key, value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }

    fn update_state(
        &mut self,
        local: &mut LocalEnv,
        external: &mut ExternalEnv,
    ) -> std::result::Result<(), ExpressionError> {
        if let MetadataKey::Query(query) = &self.key {
            let insert_type = self.value.type_def((local, external)).kind().clone();
            match query.target() {
                QueryTarget::External => {
                    let mut new_type = external.metadata_kind().clone();
                    new_type.insert(query.path(), insert_type);
                    external.update_metadata(new_type);
                }
                QueryTarget::Container(_)
                | QueryTarget::FunctionCall(_)
                | QueryTarget::Internal(_) => unreachable!("only external queries are allowed"),
            }
        }
        Ok(())
    }
}
