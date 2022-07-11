use std::collections::{BTreeMap, BTreeSet};

use crate::config::LogNamespace;
use lookup::LookupBuf;
use value::kind::insert;
use value::{
    kind::{merge, Collection},
    Kind,
};

/// The definition of a schema.
///
/// This struct contains all the information needed to inspect the schema of an event emitted by
/// a source/transform.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Definition {
    /// The type of the event
    kind: Kind,

    /// Semantic meaning assigned to fields within the collection.
    ///
    /// The value within this map points to a path inside the `collection`.
    meaning: BTreeMap<String, MeaningPointer>,

    /// Type definitions of components can change depending on the log namespace chosen.
    /// This records which ones are possible.
    /// An empty set means the definition can't be for a log
    log_namespaces: BTreeSet<LogNamespace>,
}

/// In regular use, a semantic meaning points to exactly _one_ location in the collection. However,
/// when merging two [`Definition`]s, we need to be able to allow for two definitions with the same
/// semantic meaning identifier to be merged together.
///
/// We cannot error when this happens, because a follow-up component (such as the `remap`
/// transform) might rectify the issue of having a semantic meaning with multiple pointers.
///
/// Because of this, we encapsulate this state in an enum. The schema validation step done by the
/// sink builder, will return an error if the definition stores an "invalid" meaning pointer.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
enum MeaningPointer {
    Valid(LookupBuf),
    Invalid(BTreeSet<LookupBuf>),
}

impl MeaningPointer {
    fn merge(self, other: Self) -> Self {
        let set = match (self, other) {
            (Self::Valid(lhs), Self::Valid(rhs)) if lhs == rhs => return Self::Valid(lhs),
            (Self::Valid(lhs), Self::Valid(rhs)) => BTreeSet::from([lhs, rhs]),
            (Self::Valid(lhs), Self::Invalid(mut rhs)) => {
                rhs.insert(lhs);
                rhs
            }
            (Self::Invalid(mut lhs), Self::Valid(rhs)) => {
                lhs.insert(rhs);
                lhs
            }
            (Self::Invalid(mut lhs), Self::Invalid(rhs)) => {
                lhs.extend(rhs);
                lhs
            }
        };

        Self::Invalid(set)
    }
}

#[cfg(test)]
impl From<&str> for MeaningPointer {
    fn from(v: &str) -> Self {
        MeaningPointer::Valid(v.into())
    }
}

#[cfg(test)]
impl From<LookupBuf> for MeaningPointer {
    fn from(v: LookupBuf) -> Self {
        MeaningPointer::Valid(v)
    }
}

impl Definition {
    /// The most general possible definition. The `Kind` is `any`, and all `log_namespaces` are enabled.
    pub fn any() -> Self {
        Self::new(Kind::any(), [LogNamespace::Legacy, LogNamespace::Vector])
    }

    /// Creates a new definition that is of the kind specified.
    /// There are no meanings.
    /// The `log_namespaces` are used to list the possible namespaces the schema is for.
    pub fn new(kind: Kind, log_namespaces: impl Into<BTreeSet<LogNamespace>>) -> Self {
        Self {
            kind,
            meaning: BTreeMap::default(),
            log_namespaces: log_namespaces.into(),
        }
    }

    /// An object with any fields, and the `Legacy` namespace.
    /// This is the default schema for a source that does not explicitely provide one yet.
    pub fn default_legacy_namespace() -> Self {
        Self::new(Kind::any_object(), [LogNamespace::Legacy])
    }

    /// An object with no fields, and the `Legacy` namespace.
    /// This is what most sources use for the legacy namespace.
    pub fn empty_legacy_namespace() -> Self {
        Self::new(Kind::object(Collection::empty()), [LogNamespace::Legacy])
    }

    /// Returns the source schema for a source that produce the listed log namespaces,
    /// but an explicit schema was not provided.
    pub fn default_for_namespace(log_namespaces: &BTreeSet<LogNamespace>) -> Self {
        let is_legacy = log_namespaces.contains(&LogNamespace::Legacy);
        let is_vector = log_namespaces.contains(&LogNamespace::Vector);
        match (is_legacy, is_vector) {
            (false, false) => Self::new(Kind::any(), []),
            (true, false) => Self::default_legacy_namespace(),
            (false, true) => Self::new(Kind::any(), [LogNamespace::Vector]),
            (true, true) => Self::any(),
        }
    }

    /// The set of possible log namespaces that events can use. When merged, this is the union of all inputs.
    pub fn log_namespaces(&self) -> &BTreeSet<LogNamespace> {
        &self.log_namespaces
    }

    /// Add type information for an event field.
    /// A non-root required field means the root type must be an object, so the type will be automatically
    /// restricted to an object.
    ///
    /// # Panics
    /// - If the path is not root, and the definition does not allow the type to be an object.
    /// - Provided path has one or more coalesced segments (e.g. `.(foo | bar)`).
    #[must_use]
    pub fn with_field(
        mut self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        meaning: Option<&str>,
    ) -> Self {
        let path = path.into();
        let meaning = meaning.map(ToOwned::to_owned);

        if !path.is_root() {
            if kind.contains_null() {
                // field is optional, so don't coerce to an object, but still make sure it _can_ be an object
                assert!(
                    self.kind.as_object().is_some(),
                    "Setting a field on a value that cannot be an object"
                );
            } else {
                self.kind = self
                    .kind
                    .into_object()
                    .expect("required field implies the type can be an object")
                    .into();
            }
        }

        self.kind
            .insert_at_path(
                &path.to_lookup(),
                kind,
                insert::Strategy {
                    inner_conflict: insert::InnerConflict::Replace,
                    leaf_conflict: insert::LeafConflict::Replace,
                    coalesced_path: insert::CoalescedPath::Reject,
                },
            )
            .expect("Field definition not valid");

        if let Some(meaning) = meaning {
            self.meaning.insert(meaning, MeaningPointer::Valid(path));
        }

        self
    }

    /// Add type information for an optional event field.
    ///
    /// # Panics
    ///
    /// See `Definition::require_field`.
    #[must_use]
    pub fn optional_field(
        self,
        path: impl Into<LookupBuf>,
        kind: Kind,
        meaning: Option<&str>,
    ) -> Self {
        self.with_field(path, kind.or_null(), meaning)
    }

    /// Register a semantic meaning for the definition.
    ///
    /// # Panics
    ///
    /// This method panics if the provided path points to an unknown location in the collection.
    #[must_use]
    pub fn with_meaning(mut self, path: impl Into<LookupBuf>, meaning: &str) -> Self {
        let path = path.into();

        // Ensure the path exists in the collection.
        assert!(
            self.kind
                .find_at_path(&path.to_lookup())
                .ok()
                .flatten()
                .is_some(),
            "meaning must point to a valid path"
        );

        self.meaning
            .insert(meaning.to_owned(), MeaningPointer::Valid(path));
        self
    }

    /// Set the kind for all unknown fields.
    #[must_use]
    pub fn unknown_fields(mut self, unknown: impl Into<Option<Kind>>) -> Self {
        let unknown = unknown.into();
        if let Some(object) = self.kind.as_object_mut() {
            object.set_unknown(unknown.clone());
        }
        if let Some(array) = self.kind.as_array_mut() {
            array.set_unknown(unknown);
        }
        self
    }

    /// Merge `other` definition into `self`.
    ///
    /// This just takes the union of both definitions.
    #[must_use]
    pub fn merge(mut self, mut other: Self) -> Self {
        for (other_id, other_meaning) in other.meaning {
            let meaning = match self.meaning.remove(&other_id) {
                Some(this_meaning) => this_meaning.merge(other_meaning),
                None => other_meaning,
            };

            self.meaning.insert(other_id, meaning);
        }

        self.kind.merge(
            other.kind,
            merge::Strategy {
                collisions: merge::CollisionStrategy::Union,
                indices: merge::Indices::Keep,
            },
        );

        self.log_namespaces.append(&mut other.log_namespaces);
        self
    }

    /// Returns a `Lookup` into an event, based on the provided `meaning`, if the meaning exists.
    pub fn meaning_path(&self, meaning: &str) -> Option<&LookupBuf> {
        match self.meaning.get(meaning) {
            Some(MeaningPointer::Valid(path)) => Some(path),
            None | Some(MeaningPointer::Invalid(_)) => None,
        }
    }

    pub fn invalid_meaning(&self, meaning: &str) -> Option<&BTreeSet<LookupBuf>> {
        match &self.meaning.get(meaning) {
            Some(MeaningPointer::Invalid(paths)) => Some(paths),
            None | Some(MeaningPointer::Valid(_)) => None,
        }
    }

    pub fn meanings(&self) -> impl Iterator<Item = (&String, &LookupBuf)> {
        self.meaning
            .iter()
            .filter_map(|(id, pointer)| match pointer {
                MeaningPointer::Valid(path) => Some((id, path)),
                MeaningPointer::Invalid(_) => None,
            })
    }

    pub fn kind(&self) -> &Kind {
        &self.kind
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

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
                    want: Definition {
                        kind: Kind::object(BTreeMap::from([("foo".into(), Kind::boolean())])),
                        meaning: [("foo_meaning".to_owned(), "foo".into())].into(),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
            (
                "nested fields",
                TestCase {
                    path: LookupBuf::from_str(".foo.bar").unwrap(),
                    kind: Kind::regex().or_null(),
                    meaning: Some("foobar"),
                    want: Definition {
                        kind: Kind::object(BTreeMap::from([(
                            "foo".into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::regex().or_null())])),
                        )])),
                        meaning: [(
                            "foobar".to_owned(),
                            LookupBuf::from_str(".foo.bar").unwrap().into(),
                        )]
                        .into(),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
            (
                "no meaning",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: None,
                    want: Definition {
                        kind: Kind::object(BTreeMap::from([("foo".into(), Kind::boolean())])),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
        ]) {
            let got = Definition::empty_legacy_namespace().with_field(path, kind, meaning);
            assert_eq!(got.kind(), want.kind(), "{}", title);
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
                    want: Definition {
                        kind: Kind::object(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )])),
                        meaning: [("foo_meaning".to_owned(), "foo".into())].into(),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
            (
                "nested fields",
                TestCase {
                    path: LookupBuf::from_str(".foo.bar").unwrap(),
                    kind: Kind::regex().or_null(),
                    meaning: Some("foobar"),
                    want: Definition {
                        kind: Kind::object(BTreeMap::from([(
                            "foo".into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::regex().or_null())])),
                        )])),
                        meaning: [(
                            "foobar".to_owned(),
                            LookupBuf::from_str(".foo.bar").unwrap().into(),
                        )]
                        .into(),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
            (
                "no meaning",
                TestCase {
                    path: "foo".into(),
                    kind: Kind::boolean(),
                    meaning: None,
                    want: Definition {
                        kind: Kind::object(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )])),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
        ]) {
            let mut got = Definition::new(Kind::object(BTreeMap::new()), []);
            got = got.optional_field(path, kind, meaning);

            assert_eq!(got, want, "{}", title);
        }
    }

    #[test]
    fn test_unknown_fields() {
        let want = Definition {
            kind: Kind::object(Collection::from_unknown(Kind::bytes().or_integer())),
            meaning: BTreeMap::default(),
            log_namespaces: BTreeSet::new(),
        };

        let mut got = Definition::new(Kind::object(Collection::empty()), []);
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
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )]))),
                        meaning: BTreeMap::from([("foo_meaning".to_owned(), "foo".into())]),
                        log_namespaces: BTreeSet::new(),
                    },
                    other: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )]))),
                        meaning: BTreeMap::from([("foo_meaning".to_owned(), "foo".into())]),
                        log_namespaces: BTreeSet::new(),
                    },
                    want: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )]))),
                        meaning: BTreeMap::from([("foo_meaning".to_owned(), "foo".into())]),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
            (
                "this optional, other required",
                TestCase {
                    this: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )]))),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                    other: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                    want: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )]))),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
            (
                "this required, other optional",
                TestCase {
                    this: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                    other: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )]))),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                    want: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean().or_null(),
                        )]))),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
            (
                "this required, other required",
                TestCase {
                    this: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                    other: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                    want: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::default(),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
            (
                "same meaning, pointing to different paths",
                TestCase {
                    this: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("foo".into()),
                        )]),
                        log_namespaces: BTreeSet::new(),
                    },
                    other: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("bar".into()),
                        )]),
                        log_namespaces: BTreeSet::new(),
                    },
                    want: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Invalid(BTreeSet::from(["foo".into(), "bar".into()])),
                        )]),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
            (
                "same meaning, pointing to same path",
                TestCase {
                    this: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("foo".into()),
                        )]),
                        log_namespaces: BTreeSet::new(),
                    },
                    other: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("foo".into()),
                        )]),
                        log_namespaces: BTreeSet::new(),
                    },
                    want: Definition {
                        kind: Kind::object(Collection::from(BTreeMap::from([(
                            "foo".into(),
                            Kind::boolean(),
                        )]))),
                        meaning: BTreeMap::from([(
                            "foo".into(),
                            MeaningPointer::Valid("foo".into()),
                        )]),
                        log_namespaces: BTreeSet::new(),
                    },
                },
            ),
        ]) {
            let got = this.merge(other);

            assert_eq!(got, want, "{}", title);
        }
    }
}
