use std::collections::{HashMap, HashSet};

use lookup::LookupBuf;
use value::{
    kind::{merge, nest, Collection, Field, Unknown},
    Kind,
};

/// The definition of a schema.
///
/// This struct contains all the information needed to inspect the schema of an event emitted by
/// a source/transform.
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    /// The collection of fields and their types stored in the event.
    collection: Collection<Field>,

    /// Semantic meaning assigned to fields within the collection.
    ///
    /// The value within this map points to a path inside the `collection`. It is an invalid state
    /// for there to be a meaning pointing to a non-existing path in the collection.
    meaning: HashMap<String, LookupBuf>,

    /// A list of paths that are allowed to be missing.
    ///
    /// The key in this set points to a path inside the `collection`. It is an invalid state for
    /// there to be a key pointing to a non-existing path in the collection.
    optional: HashSet<LookupBuf>,
}

impl Definition {
    /// Create an "empty" output schema.
    ///
    /// This means no type information is known about the event.
    pub fn empty() -> Self {
        Self {
            collection: Collection::empty(),
            meaning: HashMap::default(),
            optional: HashSet::default(),
        }
    }

    /// Check if the definition is "empty", meaning:
    ///
    /// 1. There are no known fields defined.
    /// 2. The unknown fields are set to "any".
    pub fn is_empty(&self) -> bool {
        self.collection.known().is_empty()
            && self.collection.unknown().map_or(false, Unknown::is_any)
    }

    /// Add type information for an event field.
    ///
    /// # Panics
    ///
    /// - Provided path is a root path (e.g. `.`).
    /// - Provided path points to a root-level array (e.g. `.[0]`).
    /// - Provided path has one or more coalesced segments (e.g. `.(foo | bar)`).
    pub fn required_field(
        mut self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        meaning: Option<&'static str>,
    ) -> Self {
        let mut path = path.into();
        let meaning = meaning.map(ToOwned::to_owned);

        match path.get(0) {
            None => panic!("must not be a root path"),
            Some(segment) if segment.is_index() => panic!("must not start with an index"),
            _ => {}
        };

        let collection = kind
            .nest_at_path(
                &path.to_lookup(),
                nest::Strategy {
                    coalesced_path: nest::CoalescedPath::Reject,
                },
            )
            .expect("non-coalesced path used")
            .into_object()
            .expect("always object");

        self.collection.merge(
            collection,
            merge::Strategy {
                depth: merge::Depth::Deep,
                indices: merge::Indices::Keep,
            },
        );

        if let Some(meaning) = meaning {
            self.meaning.insert(meaning, path);
        }

        self
    }

    /// Add type information for an optional event field.
    ///
    /// # Panics
    ///
    /// See `Definition::require_field`.
    pub fn optional_field(
        mut self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        meaning: Option<&'static str>,
    ) -> Self {
        let path = path.into();
        self.optional.insert(path.clone());

        self.required_field(path, kind, meaning)
    }

    /// Set the kind for all unknown fields.
    pub fn unknown_fields(mut self, unknown: impl Into<Option<Kind>>) -> Self {
        self.collection.set_unknown(unknown);
        self
    }

    /// Merge `other` definition into `self`.
    ///
    /// The merge strategy for optional fields is as follows:
    ///
    /// If the field is marked as optional in both definitions, _or_ if it's optional in one,
    /// and unspecified in the other, then the field remains optional.
    ///
    /// If it's marked as "required" in either of the two definitions, then it becomes
    /// a required field in the merged definition.
    ///
    /// Note that it is allowed to have required field nested under optional fields. For
    /// example, `.foo` might be set as optional, but `.foo.bar` as required. In this case, it
    /// means that the object at `.foo` is allowed to be missing, but if it's present, then it's
    /// required to have a `bar` field.
    pub fn merge(mut self, other: Self) -> Self {
        let mut optional = HashSet::default();

        for path in &self.optional {
            if other.is_optional_field(path)
                || other
                    .collection
                    .find_known_at_path(&mut path.to_lookup())
                    .ok()
                    .flatten()
                    .is_none()
            {
                optional.insert(path.clone());
            }
        }
        for path in other.optional {
            if self.is_optional_field(&path)
                || self
                    .collection
                    .find_known_at_path(&mut path.to_lookup())
                    .ok()
                    .flatten()
                    .is_none()
            {
                optional.insert(path);
            }
        }

        self.optional = optional;
        self.meaning.extend(other.meaning);
        self.collection.merge(
            other.collection,
            merge::Strategy {
                depth: merge::Depth::Deep,
                indices: merge::Indices::Keep,
            },
        );

        self
    }

    /// Returns `true` if the provided field is marked as optional.
    fn is_optional_field(&self, path: &LookupBuf) -> bool {
        self.optional.contains(path)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn test_required_field() {
        struct TestCase {
            path: LookupBuf,
            kind: Kind,
            meaning: Option<&'static str>,
            want: Definition,
        }

        for (
            title,
            TestCase {
                path,
                kind,
                meaning,
                want,
            },
        ) in HashMap::from([
            (
                "simple",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: Some("foo_meaning"),
                    want: {
                        let collection =
                            Collection::from(BTreeMap::from([("foo".into(), Kind::boolean())]));
                        let meaning = HashMap::from([("foo_meaning".to_owned(), "foo".into())]);
                        let optional = HashSet::default();

                        Definition {
                            collection,
                            meaning,
                            optional,
                        }
                    },
                },
            ),
            (
                "nested fields",
                TestCase {
                    path: LookupBuf::from_str(".foo.bar").unwrap(),
                    kind: Kind::regex().or_null(),
                    meaning: Some("foobar"),
                    want: {
                        let collection = Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::regex().or_null())])),
                        )]));
                        let meaning = HashMap::from([(
                            "foobar".to_owned(),
                            LookupBuf::from_str(".foo.bar").unwrap(),
                        )]);
                        let optional = HashSet::default();

                        Definition {
                            collection,
                            meaning,
                            optional,
                        }
                    },
                },
            ),
            (
                "no meaning",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: None,
                    want: {
                        let collection =
                            Collection::from(BTreeMap::from([("foo".into(), Kind::boolean())]));
                        let meaning = HashMap::default();
                        let optional = HashSet::default();

                        Definition {
                            collection,
                            meaning,
                            optional,
                        }
                    },
                },
            ),
        ]) {
            let mut got = Definition::empty();
            got = got.required_field(path, kind, meaning);

            assert_eq!(got, want, "{}", title);
        }
    }

    #[test]
    fn test_optional_field() {
        struct TestCase {
            path: LookupBuf,
            kind: Kind,
            meaning: Option<&'static str>,
            want: Definition,
        }

        for (
            title,
            TestCase {
                path,
                kind,
                meaning,
                want,
            },
        ) in HashMap::from([
            (
                "simple",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: Some("foo_meaning"),
                    want: {
                        let collection =
                            Collection::from(BTreeMap::from([("foo".into(), Kind::boolean())]));
                        let meaning = HashMap::from([("foo_meaning".to_owned(), "foo".into())]);
                        let optional = HashSet::from(["foo".into()]);

                        Definition {
                            collection,
                            meaning,
                            optional,
                        }
                    },
                },
            ),
            (
                "nested fields",
                TestCase {
                    path: LookupBuf::from_str(".foo.bar").unwrap(),
                    kind: Kind::regex().or_null(),
                    meaning: Some("foobar"),
                    want: {
                        let collection = Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::regex().or_null())])),
                        )]));
                        let meaning = HashMap::from([(
                            "foobar".to_owned(),
                            LookupBuf::from_str(".foo.bar").unwrap(),
                        )]);
                        let optional = HashSet::from([LookupBuf::from_str(".foo.bar").unwrap()]);

                        Definition {
                            collection,
                            meaning,
                            optional,
                        }
                    },
                },
            ),
            (
                "no meaning",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: None,
                    want: {
                        let collection =
                            Collection::from(BTreeMap::from([("foo".into(), Kind::boolean())]));
                        let meaning = HashMap::default();
                        let optional = HashSet::from(["foo".into()]);

                        Definition {
                            collection,
                            meaning,
                            optional,
                        }
                    },
                },
            ),
        ]) {
            let mut got = Definition::empty();
            got = got.optional_field(path, kind, meaning);

            assert_eq!(got, want, "{}", title);
        }
    }

    #[test]
    fn test_unknown_fields() {
        let want = Definition {
            collection: Collection::from_unknown(Kind::bytes().or_integer()),
            meaning: HashMap::default(),
            optional: HashSet::default(),
        };

        let mut got = Definition::empty();
        got = got.unknown_fields(Kind::boolean());
        got = got.unknown_fields(Kind::bytes().or_integer());

        assert_eq!(got, want);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_merge() {
        struct TestCase {
            this: Definition,
            other: Definition,
            want: Definition,
        }

        for (title, TestCase { this, other, want }) in HashMap::from([
            (
                "equal definitions",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::from(["foo".into()]),
                        meaning: HashMap::from([("foo_meaning".to_owned(), "foo".into())]),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::from(["foo".into()]),
                        meaning: HashMap::from([("foo_meaning".to_owned(), "foo".into())]),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::from(["foo".into()]),
                        meaning: HashMap::from([("foo_meaning".to_owned(), "foo".into())]),
                    },
                },
            ),
            (
                "this optional, other required",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::from(["foo".into()]),
                        meaning: HashMap::default(),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::default(),
                        meaning: HashMap::default(),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::default(),
                        meaning: HashMap::default(),
                    },
                },
            ),
            (
                "this required, other optional",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::default(),
                        meaning: HashMap::default(),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::from(["foo".into()]),
                        meaning: HashMap::default(),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::default(),
                        meaning: HashMap::default(),
                    },
                },
            ),
            (
                "this required, other required",
                TestCase {
                    this: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::default(),
                        meaning: HashMap::default(),
                    },
                    other: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::default(),
                        meaning: HashMap::default(),
                    },
                    want: Definition {
                        collection: Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )])),
                        optional: HashSet::default(),
                        meaning: HashMap::default(),
                    },
                },
            ),
        ]) {
            let got = this.merge(other);

            assert_eq!(got, want, "{}", title);
        }
    }
}
