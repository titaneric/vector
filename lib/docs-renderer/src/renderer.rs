use std::collections::{HashMap, VecDeque};

use anyhow::Result;
use serde::Serialize;
use serde_json::{Map, Value};
use snafu::Snafu;
use tracing::debug;
use vector_config::schema::{
    parser::query::{OneOrMany, QueryError, QueryableSchema, SchemaQuerier, SchemaType},
    visitors::merge::Mergeable,
    InstanceType,
};
use vector_config_common::constants;

#[derive(Debug, Snafu)]
pub enum RenderError {
    #[snafu(display("rendering failed: {reason}"))]
    Failed { reason: String },

    #[snafu(display("query error during rendering: {source}"), context(false))]
    Query { source: QueryError },
}

#[derive(Serialize)]
#[serde(transparent)]
pub struct RenderData {
    root: Value,
}

impl RenderData {
    fn with_mut_object<F, V>(&mut self, f: F) -> V
    where
        F: FnOnce(&mut Map<String, Value>) -> V,
    {
        // TODO: We should refactor this method so that it takes the desired path, a boolean for
        // whether or not to create missing path nodes, and a closure to call with the object
        // reference/object key if it exists.. and then this way, `write` and `delete` become simple
        // calls with simple closures that just do `map.insert(...)` and `map.delete(...)` and so
        // on.
        //
        // tl;dr: make it DRY.
        let map = self
            .root
            .as_object_mut()
            .expect("Render data should always have an object value as root.");
        f(map)
    }

    /// Writes a value at the given path.
    ///
    /// The path follows the form of `/part1/part/.../partN`, where each slash-separated segment
    /// represents a nested object within the overall object hierarchy. For example, a path of
    /// `/root/nested/key2` would map to the value "weee!" if applied against the following JSON
    /// object:
    ///
    ///   { "root": { "nested": { "key2": "weee!" } } }
    ///
    /// # Panics
    ///
    /// If the path does not start with a forward slash, this method will panic. Likewise, if the
    /// path is _only_ a forward slash (aka there is no segment to describe the key within the
    /// object to write the value to), this method will panic.
    ///
    /// If any nested object within the path does not yet exist, it will be created. If any segment,
    /// other than the leaf segment, points to a value that is not an object/map, this method will
    /// panic.
    pub fn write<V: Into<Value>>(&mut self, path: &str, value: V) {
        if !path.starts_with('/') {
            panic!("Paths must always start with a leading forward slash (`/`).");
        }

        self.with_mut_object(|map| {
            // Split the path, and take the last element as the actual map key to write to.
            let mut segments = path.split('/').collect::<VecDeque<_>>();
            // Remove the empty string that comes from the leading slash.
            segments.pop_front();
            let key = segments.pop_back().expect("Path must end with a key.");

            // Iterate over the remaining elements, traversing into the root object one level at a
            // time, based on using `token` as the map key. If there's no map at the given key,
            // we'll create one. If there's something other than a map, we'll panic.
            let mut destination = map;
            while let Some(segment) = segments.pop_front() {
                if destination.contains_key(segment) {
                    match destination.get_mut(segment) {
                        Some(Value::Object(ref mut next)) => {
                            destination = next;
                            continue;
                        }
                        Some(_) => {
                            panic!("Only leaf nodes should be allowed to be non-object values.")
                        }
                        None => unreachable!("Already asserted that the given key exists."),
                    }
                } else {
                    destination.insert(segment.to_string(), Value::Object(Map::new()));
                    match destination.get_mut(segment) {
                        Some(Value::Object(ref mut next)) => {
                            destination = next;
                        }
                        _ => panic!("New object was just inserted."),
                    }
                }
            }

            destination.insert(key.to_string(), value.into());
        });
    }

    /// Deletes the value at the given path.
    ///
    /// The path follows the form of `/part1/part/.../partN`, where each slash-separated segment
    /// represents a nested object within the overall object hierarchy. For example, a path of
    /// `/root/nested/key2` would map to the value "weee!" if applied against the following JSON
    /// object:
    ///
    ///   { "root": { "nested": { "key2": "weee!" } } }
    ///
    /// # Panics
    ///
    /// If the path does not start with a forward slash, this method will panic. Likewise, if the
    /// path is _only_ a forward slash (aka there is no segment to describe the key within the
    /// object to write the value to), this method will panic.
    ///
    /// If any nested object within the path does not yet exist, it will be created. If any segment,
    /// other than the leaf segment, points to a value that is not an object/map, this method will
    /// panic.
    pub fn delete(&mut self, path: &str) -> bool {
        if !path.starts_with('/') {
            panic!("Paths must always start with a leading forward slash (`/`).");
        }

        self.with_mut_object(|map| {
            // Split the path, and take the last element as the actual map key to write to.
            let mut segments = path.split('/').collect::<VecDeque<_>>();
            // Remove the empty string that comes from the leading slash.
            segments.pop_front();
            let key = segments
                .pop_back()
                .expect("Path cannot point directly to the root. Use `clear` instead.");

            // Iterate over the remaining elements, traversing into the root object one level at a
            // time, based on using `token` as the map key. If there's no map at the given key,
            // we'll create one. If there's something other than a map, we'll panic.
            let mut destination = map;
            while let Some(segment) = segments.pop_front() {
                match destination.get_mut(segment) {
                    Some(Value::Object(ref mut next)) => {
                        destination = next;
                        continue;
                    }
                    Some(_) => panic!("Only leaf nodes should be allowed to be non-object values."),
                    // If the next segment doesn't exist, there's nothing for us to delete, so return `false`.
                    None => return false,
                }
            }

            destination.remove(key).is_some()
        })
    }

    /// Gets whether or not a value at the given path.
    ///
    /// The path follows the form of `/part1/part/.../partN`, where each slash-separated segment
    /// represents a nested object within the overall object hierarchy. For example, a path of
    /// `/root/nested/key2` would map to the value "weee!" if applied against the following JSON
    /// object:
    ///
    ///   { "root": { "nested": { "key2": "weee!" } } }
    ///
    /// # Panics
    ///
    /// If the path does not start with a forward slash, this method will panic.
    pub fn exists(&self, path: &str) -> bool {
        if !path.starts_with('/') {
            panic!("Paths must always start with a leading forward slash (`/`).");
        }

        // The root path always exists.
        if path == "/" {
            return true;
        }

        self.root.pointer(path).is_some()
    }

    /// Merges object from `other` into `self`.
    ///
    /// Uses a "deep" merge strategy, which will recursively merge both objects together. This
    /// strategy behaves as follows:
    ///
    /// - strings, booleans, integers, numbers, and nulls are "highest priority wins" (`self` has
    ///   highest priority)
    /// - arrays are merged together without any deduplication, with the items from `self` appearing
    ///   first
    /// - objects have their properties merged together, but if an overlapping property is
    ///   encountered:
    ///   - if it has the same type on both sides, the property is merged normally (using the
    ///     standard merge behavior)
    ///   - if it does not have the same type on both sides, the property value on the `self` side
    ///     takes precedence
    ///
    /// The only exception to the merge behavior above is if an overlapping object property does not
    /// have the same type on both sides, but the type on the `self` side is an array. When the type
    /// is an array, the value on the `other` side is appended to that array, regardless of the
    /// contents of the array.
    pub fn merge(&mut self, other: Self) {
        if self.root.is_null() {
            self.root = other.root;
            return;
        } else if other.root.is_null() {
            return;
        } else if self.mergeable(&self.root) && self.mergeable(&other.root) {
            let mut self_root = self.root.clone();
            self.nested_merge(&mut self_root, &other.root);
            self.root = self_root;
        }
    }

    fn mergeable(&self, value: &Value) -> bool {
        value.is_array() || value.is_object()
    }

    fn nested_merge(&self, base: &mut Value, other: &Value) {
        let mut base_clone = base.clone();
        match (&mut base_clone, other) {
            (Value::Object(self_obj), Value::Object(other_obj)) => {
                for (k, v) in other_obj {
                    self.nested_merge(self_obj.entry(k).or_insert(Value::Null), v);
                }
                *base.as_object_mut().unwrap() = self_obj.clone();
            }
            (Value::Array(self_array), Value::Array(other_array)) => {
                self_array.extend(other_array.clone());
                *base.as_array_mut().unwrap() = self_array.clone();
            }
            _ => {
                *base = other.clone();
            }
        }
    }
}

impl Default for RenderData {
    fn default() -> Self {
        Self {
            root: Value::Object(Map::new()),
        }
    }
}

pub struct SchemaRenderer<'a, T> {
    querier: &'a SchemaQuerier,
    schema: T,
    data: RenderData,
}

impl<'a, T> SchemaRenderer<'a, T>
where
    T: QueryableSchema,
{
    pub fn new(querier: &'a SchemaQuerier, schema: T) -> Self {
        Self {
            querier,
            schema,
            data: RenderData::default(),
        }
    }

    pub fn render(self) -> Result<RenderData, RenderError> {
        let Self {
            querier,
            schema,
            mut data,
        } = self;

        // If a schema is hidden, then we intentionally do not want to render it.
        if schema.has_flag_attribute(constants::DOCS_META_HIDDEN)? {
            debug!("Schema is marked as hidden. Skipping rendering.");

            return Ok(data);
        }

        // If a schema has an overridden type, we return some barebones render data.
        if schema.has_flag_attribute(constants::DOCS_META_TYPE_OVERRIDE)? {
            debug!("Schema has overridden type.");

            data.write("/type", "blank");
            apply_schema_description(&schema, &mut data)?;

            return Ok(data);
        }

        // Now that we've handled any special cases, attempt to render the schema.
        render_bare_schema(querier, &schema, &mut data)?;

        // If the rendered schema represents an array schema, remove any description that is present
        // for the schema of the array items themselves. We want the description of whatever object
        // property that is using this array schema to be the one that is used.
        //
        // We just do this blindly because the control flow doesn't change depending on whether or
        // not it's an array schema and we do or don't delete anything.
        if data.delete("/type/array/items/description") {
            debug!("Cleared description for items schema from top-level array schema.");
        }

        // Apply any necessary defaults, descriptions, and so on, to the rendered schema.
        //
        // This must happen here because there could be callsite-specific overrides to default
        // values/descriptions/etc which must take precedence, so that must occur after any nested
        // rendering in order to maintain that precedence.
        apply_schema_default_value(&schema, &mut data)?;
        apply_schema_metadata(&schema, &mut data)?;
        apply_schema_description(&schema, &mut data)?;

        Ok(data)
    }
}

fn render_bare_schema<T: QueryableSchema>(
    querier: &SchemaQuerier,
    schema: T,
    data: &mut RenderData,
) -> Result<(), RenderError> {
    match schema.schema_type() {
        SchemaType::AllOf(subschemas) => {
            // Composite (`allOf`) schemas are indeed the sum of all of their parts, so render each
            // subschema and simply merge the rendered subschemas together.
            for subschema in subschemas {
                println!("{:?}", subschema);
                let subschema_renderer = SchemaRenderer::new(querier, subschema);
                let rendered_subschema = subschema_renderer.render()?;
                println!("{:?}", rendered_subschema.root);
                data.merge(rendered_subschema);
                println!("---");
            }
        }
        SchemaType::OneOf(_subschemas) => {}
        SchemaType::AnyOf(_subschemas) => {}
        SchemaType::Constant(const_value) => {
            // All we need to do is figure out the rendered type for the constant value, so we can
            // generate the right type path and stick the constant value in it.
            let rendered_const_type = get_rendered_value_type(&schema, const_value)?;
            let const_type_path = format!("/type/{}/const", rendered_const_type);
            data.write(const_type_path.as_str(), const_value.clone());
        }
        SchemaType::Enum(enum_values) => {
            // Similar to constant schemas, we just need to figure out the rendered type for each
            // enum value, so that we can group them together and then write the grouped values to
            // each of their respective type paths.
            let mut type_map = HashMap::new();

            for enum_value in enum_values {
                let rendered_enum_type = get_rendered_value_type(&schema, enum_value)?;
                let type_group_entry = type_map.entry(rendered_enum_type).or_insert_with(Vec::new);
                type_group_entry.push(enum_value.clone());
            }

            let structured_type_map = type_map
                .into_iter()
                .map(|(key, values)| {
                    let mut nested = Map::new();
                    nested.insert("enum".into(), Value::Array(values));

                    (key, Value::Object(nested))
                })
                .collect::<Map<_, _>>();

            data.write("/type", structured_type_map);
        }
        SchemaType::Typed(_instance_types) => {
            // TODO: Technically speaking, we could have multiple instance types declared here,
            // which is _entirely_ valid for JSON Schema. The trick is simply that we'll likely want
            // to do something equivalent to how we handle composite schemas where we just render
            // the schema in the context of each instance type, and then merge that rendered data
            // together.
            //
            // This means that we'll need another render method that operates on a schema + instance
            // type basis, since trying to do it all in `render_bare_schema` would get ugly fast.
            //
            // Practically, all of this is fine for regular ol' data types because they don't
            // intersect, but the tricky bit would be if we encountered the null instance type. It's
            // a real/valid data type, but the main problem is that there's nothing that really
            // makes sense to do with it.
            //
            // An object property, for example, that can be X or null, is essentially an optional
            // field. We handle that by including, or excluding, that property from the object's
            // required fields, which is specific to object.
            //
            // The only real world scenario where we would theoretically hit that is for an untagged
            // enum, as a unit variant in an untagged enum is represented by `null` in JSON, in
            // terms of its serialized value. _However_, we only generate enums as `oneOf`/`anyOf`
            // schemas, so the `null` instance type should only ever show up by itself.
            //
            // Long story short, we can likely have a hard-coded check that rejects any "X or null"
            // instance type groupings, knowing that _we_ never generate schemas like that, but it's
            // still technically possible in a real-world JSON Schema document... so we should at
            // least make the error message half-way decent so that it explains as much.
        }
    }

    Ok(())
}

// fn render_typed_schema<T: QueryableSchema>(
//     querier: &SchemaQuerier,
//     schema: T,
//     data: &mut RenderData,
//     instance_type: OneOrMany<InstanceType>,
// ) -> Result<RenderData, RenderError> {
//     match instance_type {
//         OneOrMany::One(instance_type) => match instance_type {
//             InstanceType::Array => {}
//             InstanceType::Boolean => {}
//             InstanceType::Integer => {}
//             InstanceType::Null => {}
//             InstanceType::Number => {}
//             InstanceType::Object => {}
//             InstanceType::String => {

//             }
//         },
//         OneOrMany::Many(instance_types) => {}
//     }
// }

fn apply_schema_default_value<T: QueryableSchema>(
    _schema: T,
    _data: &mut RenderData,
) -> Result<(), RenderError> {
    Ok(())
}

fn apply_schema_metadata<T: QueryableSchema>(
    schema: T,
    data: &mut RenderData,
) -> Result<(), RenderError> {
    // If the schema is marked as being templateable, update the syntax of the string type field to
    // use the special `template` sentinel value, which drives template-specific logic during the
    // documentation generation phase.
    if schema.has_flag_attribute(constants::DOCS_META_TEMPLATEABLE)? && data.exists("/type/string")
    {
        data.write("/type/string/syntax", "template");
    }

    // TODO: Add examples.
    // TODO: Add units.
    // TODO: Syntax override.

    Ok(())
}

fn apply_schema_description<T: QueryableSchema>(
    schema: T,
    data: &mut RenderData,
) -> Result<(), RenderError> {
    if let Some(description) = render_schema_description(schema)? {
        data.write("/description", description);
    }

    Ok(())
}

fn get_rendered_value_type<T: QueryableSchema>(
    _schema: T,
    _value: &Value,
) -> Result<String, RenderError> {
    todo!()
}

fn render_schema_description<T: QueryableSchema>(schema: T) -> Result<Option<String>, RenderError> {
    let maybe_title = schema.title();
    let maybe_description = schema.description();

    match (maybe_title, maybe_description) {
        (Some(_title), None) => Err(RenderError::Failed {
            reason: "a schema should never have a title without a description".into(),
        }),
        (None, None) => Ok(None),
        (None, Some(description)) => Ok(Some(description.trim().to_string())),
        (Some(title), Some(description)) => {
            let concatenated = format!("{}\n\n{}", title, description);
            Ok(Some(concatenated.trim().to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::BTreeMap};

    use super::*;
    use enum_dispatch::enum_dispatch;
    use serde::Serialize;
    use serde_json::json;
    use vector_config::{
        component::{
            ApiComponent, ApiDescription, GenerateConfig, GlobalOptionDescription, SinkDescription,
        },
        schema::{
            generate_root_schema, InstanceType, Metadata, ObjectValidation, RootSchema,
            SchemaGenerator, SchemaObject, SchemaSettings, SingleOrVec,
        },
        ConfigurableRef,
    };
    use vector_lib::configurable::{configurable_component, impl_generate_config_from_default};

    /// secret backends for test
    #[configurable_component(sink("all_of_test_case"))]
    #[serde(deny_unknown_fields)]
    #[derive(Clone, Debug)]
    pub struct AllOfTestCase {
        /// value1 field
        #[configurable(metadata(docs::required = true))]
        pub value1: String,

        /// value2 field
        #[configurable(derived)]
        #[serde(flatten)]
        pub value2: AllOfTestCaseInner,
    }

    impl GenerateConfig for AllOfTestCase {
        fn generate_config() -> toml::Value {
            toml::Value::try_from(AllOfTestCase {
                value1: "test".to_string(),
                value2: AllOfTestCaseInner {
                    value3: "test".to_string(),
                },
            })
            .unwrap()
        }
    }
    /// test case for one of
    #[configurable_component]
    #[derive(Clone, Debug, Default)]
    pub struct AllOfTestCaseInner {
        /// value3 field
        pub value3: String,
    }

    impl_generate_config_from_default!(AllOfTestCaseInner);

    #[test]
    fn render_data_write() {
        let mut data = RenderData::default();

        data.write("/root/nested/key2", "weee!");

        let expected = json!({
            "root": {
                "nested": {
                    "key2": "weee!"
                }
            }
        });

        assert_eq!(data.root, expected);
    }

    #[test]
    fn render_data_delete() {
        let mut data = RenderData::default();

        data.write("/root/nested/key2", "weee!");
        data.write("/root/nested/key3", "wooo!");

        assert!(data.delete("/root/nested/key2"));
        assert!(!data.delete("/root/nested/key2"));
        assert!(data.delete("/root/nested/key3"));
        assert!(!data.delete("/root/nested/key3"));
    }

    #[test]
    fn render_data_exists() {
        let mut data = RenderData::default();

        data.write("/root/nested/key2", "weee!");

        assert!(data.exists("/root/nested/key2"));
        assert!(!data.exists("/root/nested/key3"));
    }

    #[test]
    fn render_data_merge() {
        let mut data = RenderData::default();

        data.write("/root/nested/key2", "weee!");

        let mut other = RenderData::default();
        other.write("/root/nested/key3", "wooo!");

        data.merge(other);

        let expected = json!({
            "root": {
                "nested": {
                    "key2": "weee!",
                    "key3": "wooo!"
                }
            }
        });

        assert_eq!(data.root, expected);
    }

    #[test]
    fn render_data_merge_with_array() {
        let mut data = RenderData::default();

        data.write("/root/nested/key2", "weee!");

        let mut other = RenderData::default();
        other.write("/root/nested/key3", "wooo!");

        data.write("/root/nested/key4", vec![1, 2, 3]);

        data.merge(other);

        let expected = json!({
            "root": {
                "nested": {
                    "key2": "weee!",
                    "key3": "wooo!",
                    "key4": [1, 2, 3]
                }
            }
        });
        assert_eq!(data.root, expected);
    }
    #[test]
    fn render_data_merge_extend_array() {
        let mut data = RenderData::default();

        data.write("/root/nested/key2", "weee!");
        data.write("/root/nested/key4", vec![1, 2, 3]);

        let mut other = RenderData::default();
        other.write("/root/nested/key3", "wooo!");
        other.write("/root/nested/key4", vec![4, 5, 6]);

        data.merge(other);

        let expected = json!({
            "root": {
                "nested": {
                    "key2": "weee!",
                    "key3": "wooo!",
                    "key4": [1, 2, 3, 4, 5, 6]
                }
            }
        });
        assert_eq!(data.root, expected);
    }
    #[test]
    fn render_bare_schema_all_of() {
        let root_schema = generate_root_schema::<AllOfTestCase>().unwrap();
        let querier = SchemaQuerier::from_root_schema(root_schema).unwrap();
        let simple_schema = querier
            .query()
            .with_custom_attribute_kv("docs::component_name", "all_of_test_case")
            .run_single()
            .unwrap();
        // for s in simple_schema {
        //     println!("{:?}", s);
        //     println!("{:?}", s.schema_type());
        //     println!("---")
        // }
        let mut rendered = RenderData::default();
        render_bare_schema(&querier, simple_schema, &mut rendered).unwrap();
        // println!("{:?}", rendered.root);

        let expected = json!({
            "description": "value2 field",
        });

        assert_eq!(rendered.root, expected);
    }
}
