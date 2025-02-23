use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fmt::Debug,
};

use serde::Serialize;
use serde_json::{Map, Value};
use snafu::Snafu;
use std::sync::{LazyLock, Mutex};
use tracing::{debug, Instrument};
use vector_config::schema::{
    self, generate_any_of_schema,
    parser::query::{OneOrMany, QueryError, QueryableSchema, SchemaQuerier, SchemaType},
    visitors::merge::Mergeable,
    InstanceType, RootSchema, Schema, SchemaObject, SingleOrVec, SubschemaValidation,
};
use vector_config_common::constants;

static EXPANDED_SCHEMA_CACHE: LazyLock<Mutex<HashMap<String, Schema>>> =
    LazyLock::new(Default::default);

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

impl From<Value> for RenderData {
    fn from(item: Value) -> Self {
        Self { root: item }
    }
}

pub struct SchemaRenderer<'a> {
    querier: &'a SchemaQuerier,
    schema: SchemaObject,
    data: RenderData,
}

impl<'a> SchemaRenderer<'a> {
    pub fn new(querier: &'a SchemaQuerier, schema: SchemaObject) -> Self {
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

        // println!("{:?}", schema);

        expand_schema_reference(querier, &schema)?;

        // If a schema is hidden, then we intentionally do not want to render it.
        if (&schema).has_flag_attribute(constants::DOCS_META_HIDDEN)? {
            debug!("Schema is marked as hidden. Skipping rendering.");

            return Ok(data);
        }

        // If a schema has an overridden type, we return some barebones render data.
        if (&schema).has_flag_attribute(constants::DOCS_META_TYPE_OVERRIDE)? {
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

fn expand_schema_reference<'a>(
    querier: &'a SchemaQuerier,
    unexpanded_schema: &SchemaObject,
) -> Result<SchemaObject, RenderError> {
    let mut schema = unexpanded_schema.clone();

    let original_title = (&unexpanded_schema).title();
    let original_description = (&unexpanded_schema).description();

    // Expand the top level schema reference, if it exists.
    let schema_ref = (&unexpanded_schema).get_reference();
    if let Some(schema_ref) = schema_ref {
        let schema_ref = schema_ref.to_string();
        let expanded_schema_ref = {
            let mut cached_schema_guard = EXPANDED_SCHEMA_CACHE.lock().unwrap();
            let cached_schema = cached_schema_guard.get(&schema_ref);
            if let Some(cached_schema) = cached_schema {
                cached_schema.clone()
            } else {
                let expanded_schema_ref = querier.query().get_schema_by_name(schema_ref.clone())?;
                cached_schema_guard.insert(schema_ref.clone(), expanded_schema_ref.clone());
                expanded_schema_ref
            }
        };
        schema = expanded_schema_ref.into();
    }

    match (&schema).schema_type() {
        SchemaType::Typed(instance_types) => {
            let instance_type = match instance_types {
                OneOrMany::One(instance_type) => instance_type,
                OneOrMany::Many(instance_types) => {
                    if let Some(instance_type) = instance_types.iter().next() {
                        instance_type.to_owned()
                    } else {
                        return Err(RenderError::Failed {
                            reason: "instance types must have at least one value".into(),
                        });
                    }
                }
            };
            match instance_type {
                // If the instance type is an array, expand the array items schema.
                InstanceType::Array => {
                    let items = schema.array().items.clone().unwrap();
                    let mut items_schema = items.into_iter();
                    if let Some(item_schema) = items_schema.next() {
                        let expanded_items_schema = expand_schema_reference(
                            querier,
                            &item_schema.to_owned().into_object(),
                        )?;
                        let expanded_items_schema: Schema = expanded_items_schema.into();
                        let expanded_items_schema: SingleOrVec<Schema> =
                            expanded_items_schema.into();
                        schema.array().items = Some(expanded_items_schema);
                    }
                }
                // If the instance type is an object, expand the object properties schemas.
                InstanceType::Object => {
                    let properties = schema.object().properties.clone();
                    schema.object().properties = properties
                        .into_iter()
                        .map(|(k, schema)| {
                            let schema_object = schema.into_object();
                            let expanded_property_schema =
                                expand_schema_reference(querier, &schema_object).unwrap();
                            (k, expanded_property_schema.into())
                        })
                        .collect::<BTreeMap<String, Schema>>();
                }
                _ => {}
            }
        }
        SchemaType::AllOf(subschemas)
        | SchemaType::AnyOf(subschemas)
        | SchemaType::OneOf(subschemas) => {
            let new_subschemas: Vec<Schema> = subschemas
                .into_iter()
                .map(|subschema| {
                    let schema_object = subschema.into_inner();
                    let expand_schema_object = expand_schema_reference(querier, schema_object)
                        .unwrap()
                        .into();
                    Schema::Object(expand_schema_object)
                })
                .collect::<Vec<_>>();

            if let Some(subschemas) = schema.subschemas.as_mut() {
                if let Some(_) = subschemas.all_of {
                    subschemas.all_of = Some(new_subschemas);
                } else if let Some(_) = subschemas.any_of {
                    subschemas.any_of = Some(new_subschemas);
                } else if let Some(_) = subschemas.one_of {
                    subschemas.one_of = Some(new_subschemas);
                }
            }
        }
        _ => {}
    }

    let metadata_mut = schema.metadata();
    metadata_mut.title = original_title.and_then(|s| Some(s.to_string()));
    metadata_mut.description = original_description.and_then(|s| Some(s.to_string()));

    Ok(schema)
}

fn render_bare_schema<'a>(
    querier: &'a SchemaQuerier,
    schema: &SchemaObject,
    data: &mut RenderData,
) -> Result<(), RenderError> {
    match schema.schema_type() {
        SchemaType::AllOf(subschemas) => {
            // Composite (`allOf`) schemas are indeed the sum of all of their parts, so render each
            // subschema and simply merge the rendered subschemas together.
            for subschema in subschemas {
                let subschema_renderer =
                    SchemaRenderer::new(querier, subschema.into_inner().clone());
                let rendered_subschema = subschema_renderer.render()?;
                // println!("{:?}", rendered_subschema.root);
                data.merge(rendered_subschema);
                // println!("---");
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
    use std::path::PathBuf;
    use vector_config::{
        component::{
            ApiComponent, ApiDescription, GenerateConfig, GlobalOptionDescription, SinkDescription,
        },
        schema::{
            generate_root_schema, generate_root_schema_with_settings, get_or_generate_schema,
            InstanceType, Metadata, ObjectValidation, RootSchema, Schema, SchemaGenerator,
            SchemaObject, SchemaSettings, SingleOrVec,
        },
        Configurable, ConfigurableRef,
    };
    use vector_lib::configurable::{configurable_component, impl_generate_config_from_default};

    /// array_test_case
    #[configurable_component]
    #[derive(Clone, Debug, Default)]
    pub struct ArrayTestCase(
        /// reference InnerConfig
        pub Vec<InnerConfig>,
    );
    /// object_test_case
    #[configurable_component]
    #[derive(Clone, Debug, Default)]
    pub struct ObjectTestCase {
        /// field1
        pub field1: bool,

        /// field2 should reference stdlib::PathBuf
        pub field2: PathBuf,
    }

    /// all_of_test_case
    #[configurable_component]
    #[derive(Clone, Debug, Default)]
    pub struct AllOfTestCase {
        /// field1
        pub field1: bool,

        /// inner field should reference InnerConfig
        #[serde(flatten)]
        pub inner: InnerConfig,
    }

    /// inner config
    #[configurable_component]
    #[derive(Clone, Debug, Default)]
    #[serde(deny_unknown_fields)]
    pub struct InnerConfig {
        /// field1
        pub field1: String,

        /// field2
        pub field2: bool,
    }

    /// one_of_test_case
    #[configurable_component]
    #[derive(Clone, Debug)]
    pub enum OneOfTestCase {
        /// Variant1
        Variant1,
        /// inner field should reference InnerConfig
        Variant(InnerConfig),
    }

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
    fn expand_schema_reference_array() {
        let schema_setting = SchemaSettings::default();
        let root_schema =
            generate_root_schema_with_settings::<ArrayTestCase>(schema_setting).unwrap();

        let querier = SchemaQuerier::from_root_schema(root_schema).unwrap();
        let schema_setting = SchemaSettings::default();
        let schema_generator = schema_setting.into_generator();
        let ref_schema_gen = RefCell::new(schema_generator);
        let outer_schema = ArrayTestCase::generate_schema(&ref_schema_gen).unwrap();
        match (&outer_schema).schema_type() {
            SchemaType::Typed(OneOrMany::One(InstanceType::Array)) => {}
            _ => panic!("schema type should be object"),
        }

        let expanded_schema = expand_schema_reference(&querier, &outer_schema).unwrap();
        let expanded_schema = serde_json::to_value(&expanded_schema).unwrap();
        // println!("{:?}", serde_json::to_string(&expanded_schema).unwrap());
        let expected_schema = json!(
            {
                "description": "reference InnerConfig",
                "items": {
                    "properties": {
                        "field1": {
                            "description": "field1",
                            "type": "string"
                        },
                        "field2": {
                            "description": "field2",
                            "type": "boolean"
                        }
                    },
                    "required": [
                        "field1",
                        "field2"
                    ],
                    "type": "object"
                },
                "type": "array"
            }
        );
        assert!(expanded_schema == expected_schema);
    }
    #[test]
    fn expand_schema_reference_object() {
        let schema_setting = SchemaSettings::default();
        let root_schema =
            generate_root_schema_with_settings::<ObjectTestCase>(schema_setting).unwrap();

        let querier = SchemaQuerier::from_root_schema(root_schema).unwrap();
        let schema_setting = SchemaSettings::default();
        let schema_generator = schema_setting.into_generator();
        let ref_schema_gen = RefCell::new(schema_generator);
        let outer_schema = ObjectTestCase::generate_schema(&ref_schema_gen).unwrap();
        match (&outer_schema).schema_type() {
            SchemaType::Typed(OneOrMany::One(InstanceType::Object)) => {}
            _ => panic!("schema type should be object"),
        }

        let expanded_schema = expand_schema_reference(&querier, &outer_schema).unwrap();
        let expanded_schema = serde_json::to_value(&expanded_schema).unwrap();
        let expected_schema = json!(
            {
                "properties": {
                    "field1": {
                        "description": "field1",
                        "type": "boolean"
                    },
                    "field2": {
                        "description": "field2 should reference stdlib::PathBuf",
                        "pattern": "(\\/.*|[a-zA-Z]:\\\\(?:([^<>:\"\\/\\\\|?*]*[^<>:\"\\/\\\\|?*.]\\\\|..\\\\)*([^<>:\"\\/\\\\|?*]*[^<>:\"\\/\\\\|?*.]\\\\?|..\\\\))?)",
                        "type": "string"
                    }
                },
                "required": [
                    "field1",
                    "field2"
                ],
                "type": "object"
            }
        );
        assert!(expanded_schema == expected_schema);
    }
    #[test]
    fn expand_schema_reference_all_of() {
        let schema_setting = SchemaSettings::default();
        let root_schema =
            generate_root_schema_with_settings::<AllOfTestCase>(schema_setting).unwrap();

        let querier = SchemaQuerier::from_root_schema(root_schema).unwrap();
        let schema_setting = SchemaSettings::default();
        let schema_generator = schema_setting.into_generator();
        let ref_schema_gen = RefCell::new(schema_generator);
        let outer_schema = AllOfTestCase::generate_schema(&ref_schema_gen).unwrap();
        match (&outer_schema).schema_type() {
            SchemaType::AllOf(_) => {}
            _ => panic!("schema type should be all_of"),
        }

        let expanded_schema = expand_schema_reference(&querier, &outer_schema).unwrap();
        let expanded_schema = serde_json::to_value(&expanded_schema).unwrap();
        let expected_schema = json!(
            {
                "allOf": [
                    {
                        "properties": {
                            "field1": {
                                "description": "field1",
                                "type": "boolean"
                            }
                        },
                        "required": [
                            "field1"
                        ],
                        "type": "object"
                    },
                    {
                        "description": "inner field should reference InnerConfig",
                        "properties": {
                            "field1": {
                                "description": "field1",
                                "type": "string"
                            },
                            "field2": {
                                "description": "field2",
                                "type": "boolean"
                            }
                        },
                        "required": [
                            "field1",
                            "field2"
                        ],
                        "type": "object"
                    }
                ]
            }
        );
        assert!(expanded_schema == expected_schema);
    }
    #[test]
    fn render_bare_schema_one_of() {
        let schema_setting = SchemaSettings::default();
        let root_schema =
            generate_root_schema_with_settings::<OneOfTestCase>(schema_setting).unwrap();

        let querier = SchemaQuerier::from_root_schema(root_schema).unwrap();
        let schema_setting = SchemaSettings::default();
        let schema_generator = schema_setting.into_generator();
        let ref_schema_gen = RefCell::new(schema_generator);
        let outer_schema = OneOfTestCase::generate_schema(&ref_schema_gen).unwrap();
        match (&outer_schema).schema_type() {
            SchemaType::OneOf(_) => {}
            _ => panic!("schema type should be one_of"),
        }

        let expanded_schema = expand_schema_reference(&querier, &outer_schema).unwrap();
        let expanded_schema = serde_json::to_value(&expanded_schema).unwrap();
        let expected_schema = json!(
            {
                "oneOf": [
                    {
                        "description": "Variant1",
                        "const": "Variant1",
                        "_metadata": {
                            "logical_name": "Variant1"
                        }
                    },
                    {
                        "description": "inner field should reference InnerConfig",
                        "type": "object",
                        "required": [
                            "Variant"
                        ],
                        "properties": {
                            "Variant": {
                                "description": "inner config",
                                "type": "object",
                                "required": [
                                    "field1",
                                    "field2"
                                ],
                                "properties": {
                                    "field1": {
                                        "description": "field1",
                                        "type": "string"
                                    },
                                    "field2": {
                                        "description": "field2",
                                        "type": "boolean"
                                    }
                                }
                            }
                        },
                        "_metadata": {
                            "logical_name": "Variant"
                        }
                    }
                ]
            }
        );
        assert!(expanded_schema == expected_schema);
    }
}
