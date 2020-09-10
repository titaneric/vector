use crate::event::{util, PathComponent, Value};
use serde::{Serialize, Serializer};
use std::collections::BTreeMap;
use std::iter::FromIterator;
use string_cache::DefaultAtom;

#[derive(PartialEq, Debug, Clone, Default)]
pub struct LogEvent {
    fields: BTreeMap<String, Value>,
}

impl LogEvent {
    pub fn get(&self, key: &DefaultAtom) -> Option<&Value> {
        util::log::get(&self.fields, key)
    }

    pub fn get_flat(&self, key: impl AsRef<str>) -> Option<&Value> {
        self.fields.get(key.as_ref())
    }

    pub fn get_mut(&mut self, key: &DefaultAtom) -> Option<&mut Value> {
        util::log::get_mut(&mut self.fields, key)
    }

    pub fn contains(&self, key: &DefaultAtom) -> bool {
        util::log::contains(&self.fields, key)
    }

    pub fn insert<K, V>(&mut self, key: K, value: V) -> Option<Value>
    where
        K: AsRef<str>,
        V: Into<Value>,
    {
        util::log::insert(&mut self.fields, key.as_ref(), value.into())
    }

    pub fn insert_path<V>(&mut self, key: Vec<PathComponent>, value: V) -> Option<Value>
    where
        V: Into<Value>,
    {
        util::log::insert_path(&mut self.fields, key, value.into())
    }

    pub fn insert_flat<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<Value>,
    {
        self.fields.insert(key.into(), value.into());
    }

    pub fn try_insert<V>(&mut self, key: &DefaultAtom, value: V)
    where
        V: Into<Value>,
    {
        if !self.contains(key) {
            self.insert(key.clone(), value);
        }
    }

    pub fn remove(&mut self, key: &DefaultAtom) -> Option<Value> {
        util::log::remove(&mut self.fields, &key, false)
    }

    pub fn remove_prune(&mut self, key: &DefaultAtom, prune: bool) -> Option<Value> {
        util::log::remove(&mut self.fields, &key, prune)
    }

    pub fn keys<'a>(&'a self) -> impl Iterator<Item = String> + 'a {
        util::log::keys(&self.fields)
    }

    pub fn all_fields(&self) -> impl Iterator<Item = (String, &Value)> + Serialize {
        util::log::all_fields(&self.fields)
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

impl From<BTreeMap<String, Value>> for LogEvent {
    fn from(map: BTreeMap<String, Value>) -> Self {
        LogEvent { fields: map }
    }
}

impl Into<BTreeMap<String, Value>> for LogEvent {
    fn into(self) -> BTreeMap<String, Value> {
        let Self { fields } = self;
        fields
    }
}

impl std::ops::Index<&DefaultAtom> for LogEvent {
    type Output = Value;

    fn index(&self, key: &DefaultAtom) -> &Value {
        self.get(key).expect("Key is not found")
    }
}

impl<K: Into<DefaultAtom>, V: Into<Value>> Extend<(K, V)> for LogEvent {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k.into(), v.into());
        }
    }
}

// Allow converting any kind of appropriate key/value iterator directly into a LogEvent.
impl<K: Into<DefaultAtom>, V: Into<Value>> FromIterator<(K, V)> for LogEvent {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut log_event = LogEvent::default();
        log_event.extend(iter);
        log_event
    }
}

/// Converts event into an iterator over top-level key/value pairs.
impl IntoIterator for LogEvent {
    type Item = (String, Value);
    type IntoIter = std::collections::btree_map::IntoIter<String, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields.into_iter()
    }
}

impl Serialize for LogEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_map(self.fields.iter())
    }
}
