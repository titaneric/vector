use super::Transform;
use crate::event::Event;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct LuaConfig {
    source: String,
}

#[typetag::serde(name = "lua")]
impl crate::topology::config::TransformConfig for LuaConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Lua::new(&self.source).map(|l| {
            let b: Box<dyn Transform> = Box::new(l);
            b
        })
    }
}

pub struct Lua {
    lua: rlua::Lua,
}

impl Lua {
    pub fn new(source: &str) -> Result<Self, String> {
        let lua = rlua::Lua::new();

        lua.context(|ctx| {
            let func = ctx.load(&source).into_function()?;
            ctx.set_named_registry_value("vector_func", func)?;
            Ok(())
        })
        .map_err(|err| format_error(&err))?;

        Ok(Self { lua })
    }

    fn process(&self, event: Event) -> Result<Option<Event>, rlua::Error> {
        self.lua.context(|ctx| {
            let globals = ctx.globals();

            globals.set("event", event)?;

            let func = ctx.named_registry_value::<_, rlua::Function>("vector_func")?;
            func.call(())?;

            globals.get::<_, Option<Event>>("event")
        })
    }
}

impl Transform for Lua {
    fn transform(&self, event: Event) -> Option<Event> {
        match self.process(event) {
            Ok(event) => event,
            Err(err) => {
                error!(
                    "Error in lua script; discarding event.\n{}",
                    format_error(&err)
                );
                None
            }
        }
    }
}

impl rlua::UserData for Event {
    fn add_methods<'lua, M: rlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_meta_method_mut(
            rlua::MetaMethod::NewIndex,
            |_ctx, this, (key, value): (String, Option<rlua::String<'lua>>)| {
                if let Some(string) = value {
                    this.as_mut_log()
                        .insert_explicit(key.into(), string.as_bytes().into());
                } else {
                    this.as_mut_log().remove(&key.into());
                }

                Ok(())
            },
        );

        methods.add_meta_method(rlua::MetaMethod::Index, |ctx, this, key: String| {
            if let Some(value) = this.as_log().get(&key.into()) {
                let string = ctx.create_string(&value.as_bytes())?;
                Ok(Some(string))
            } else {
                Ok(None)
            }
        });
    }
}

fn format_error(error: &rlua::Error) -> String {
    match error {
        rlua::Error::CallbackError { traceback, cause } => format_error(&cause) + "\n" + traceback,
        err => err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_error, Lua};
    use crate::{event::Event, transforms::Transform};

    #[test]
    fn lua_add_field() {
        let transform = Lua::new(
            r#"
              event["hello"] = "goodbye"
            "#,
        )
        .unwrap();

        let event = Event::from("program me");

        let event = transform.transform(event).unwrap();

        assert_eq!(event[&"hello".into()], "goodbye".into());
    }

    #[test]
    fn lua_read_field() {
        let transform = Lua::new(
            r#"
              _, _, name = string.find(event["message"], "Hello, my name is (%a+).")
              event["name"] = name
            "#,
        )
        .unwrap();

        let event = Event::from("Hello, my name is Bob.");

        let event = transform.transform(event).unwrap();

        assert_eq!(event[&"name".into()], "Bob".into());
    }

    #[test]
    fn lua_remove_field() {
        let transform = Lua::new(
            r#"
              event["name"] = nil
            "#,
        )
        .unwrap();

        let mut event = Event::new_empty_log();
        event
            .as_mut_log()
            .insert_explicit("name".into(), "Bob".into());
        let event = transform.transform(event).unwrap();

        assert!(event.as_log().get(&"name".into()).is_none());
    }

    #[test]
    fn lua_drop_event() {
        let transform = Lua::new(
            r#"
              event = nil
            "#,
        )
        .unwrap();

        let mut event = Event::new_empty_log();
        event
            .as_mut_log()
            .insert_explicit("name".into(), "Bob".into());
        let event = transform.transform(event);

        assert!(event.is_none());
    }

    #[test]
    fn lua_read_empty_field() {
        let transform = Lua::new(
            r#"
              if event["non-existant"] == nil then
                event["result"] = "empty"
              else
                event["result"] = "found"
              end
            "#,
        )
        .unwrap();

        let event = Event::new_empty_log();
        let event = transform.transform(event).unwrap();

        assert_eq!(event[&"result".into()], "empty".into());
    }

    #[test]
    fn lua_numeric_value() {
        let transform = Lua::new(
            r#"
              event["number"] = 3
            "#,
        )
        .unwrap();

        let event = transform.transform(Event::new_empty_log()).unwrap();
        assert_eq!(event[&"number".into()], "3".into());
    }

    #[test]
    fn lua_non_coercible_value() {
        let transform = Lua::new(
            r#"
              event["junk"] = {"asdf"}
            "#,
        )
        .unwrap();

        let err = transform.process(Event::new_empty_log()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("error converting Lua table to String"), err);
    }

    #[test]
    fn lua_non_string_key_write() {
        let transform = Lua::new(
            r#"
              event[false] = "hello"
            "#,
        )
        .unwrap();

        let err = transform.process(Event::new_empty_log()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("error converting Lua boolean to String"), err);
    }

    #[test]
    fn lua_non_string_key_read() {
        let transform = Lua::new(
            r#"
              print(event[false])
            "#,
        )
        .unwrap();

        let err = transform.process(Event::new_empty_log()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("error converting Lua boolean to String"), err);
    }

    #[test]
    fn lua_script_error() {
        let transform = Lua::new(
            r#"
              error("this is an error")
            "#,
        )
        .unwrap();

        let err = transform.process(Event::new_empty_log()).unwrap_err();
        let err = format_error(&err);
        assert!(err.contains("this is an error"), err);
    }

    #[test]
    fn lua_syntax_error() {
        let err = Lua::new(
            r#"
              1234 = sadf <>&*!#@
            "#,
        )
        .map(|_| ())
        .unwrap_err();

        assert!(err.contains("syntax error:"), err);
    }
}
