use crate::{Object, Path, Value};

impl Object for Value {
    fn insert(&mut self, path: &Path, value: Value) -> Result<(), String> {
        self.insert_by_path(path, value);
        Ok(())
    }

    fn get(&self, path: &Path) -> Result<Option<Value>, String> {
        Ok(self.get_by_path(path).cloned())
    }

    fn remove(&mut self, path: &Path, compact: bool) -> Result<Option<Value>, String> {
        let value = self.get(path)?;
        self.remove_by_path(path, compact);

        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{value, Field::*, Segment::*};

    #[test]
    fn object_get() {
        let cases = vec![
            (value!(true), vec![], Ok(Some(value!(true)))),
            (
                value!(true),
                vec![Field(Regular("foo".to_string()))],
                Ok(None),
            ),
            (value!({}), vec![], Ok(Some(value!({})))),
            (value!({foo: "bar"}), vec![], Ok(Some(value!({foo: "bar"})))),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("foo".to_owned()))],
                Ok(Some(value!("bar"))),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("bar".to_owned()))],
                Ok(None),
            ),
            (value!([1, 2, 3, 4, 5]), vec![Index(1)], Ok(Some(value!(2)))),
            (
                value!({foo: [{bar: true}]}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(0),
                    Field(Regular("bar".to_owned())),
                ],
                Ok(Some(value!(true))),
            ),
            (
                value!({foo: {"bar baz": {baz: 2}}}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Coalesce(vec![
                        Regular("qux".to_owned()),
                        Quoted("bar baz".to_owned()),
                    ]),
                    Field(Regular("baz".to_owned())),
                ],
                Ok(Some(value!(2))),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: Value = value;
            let path = Path::new_unchecked(segments);

            assert_eq!(value.get(&path), expect)
        }
    }

    #[test]
    fn object_insert() {
        let cases = vec![
            (
                value!({foo: "bar"}),
                vec![],
                value!({baz: "qux"}),
                value!({baz: "qux"}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("baz".to_owned()))],
                true.into(),
                value!({foo: "bar", baz: true}),
                Ok(()),
            ),
            (
                value!({foo: [{bar: "baz"}]}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(0),
                    Field(Regular("baz".to_owned())),
                ],
                true.into(),
                value!({foo: [{bar: "baz", baz: true}]}),
                Ok(()),
            ),
            (
                value!({foo: {bar: "baz"}}),
                vec![
                    Field(Regular("bar".to_owned())),
                    Field(Regular("baz".to_owned())),
                ],
                true.into(),
                value!({foo: {bar: "baz"}, bar: {baz: true}}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("foo".to_owned()))],
                "baz".into(),
                value!({foo: "baz"}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(2),
                    Field(Quoted("bar baz".to_owned())),
                    Field(Regular("a".to_owned())),
                    Field(Regular("b".to_owned())),
                ],
                true.into(),
                value!({foo: [null, null, {"bar baz": {"a": {"b": true}}}]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1, 2]}),
                vec![Field(Regular("foo".to_owned())), Index(5)],
                "baz".into(),
                value!({foo: [0, 1, 2, null, null, "baz"]}),
                Ok(()),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: []}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0]}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                value!({foo: ["baz"]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                value!({foo: ["baz", 1]}),
                Ok(()),
            ),
            (
                value!({foo: [0, 1]}),
                vec![Field(Regular("foo".to_owned())), Index(1)],
                "baz".into(),
                value!({foo: [0, "baz"]}),
                Ok(()),
            ),
        ];

        for (mut object, segments, value, expect, result) in cases {
            let path = Path::new_unchecked(segments);

            assert_eq!(Object::insert(&mut object, &path, value.clone()), result);
            assert_eq!(object, expect);
            assert_eq!(Object::get(&object, &path), Ok(Some(value)));
        }
    }

    #[test]
    fn object_remove() {
        let cases = vec![
            (
                value!({foo: "bar"}),
                vec![Field(Regular("baz".to_owned()))],
                false,
                None,
                Some(value!({foo: "bar"})),
            ),
            (
                value!({foo: "bar"}),
                vec![Field(Regular("foo".to_owned()))],
                false,
                Some(value!("bar")),
                Some(value!({})),
            ),
            (
                value!({foo: "bar"}),
                vec![Coalesce(vec![
                    Quoted("foo bar".to_owned()),
                    Regular("foo".to_owned()),
                ])],
                false,
                Some(value!("bar")),
                Some(value!({})),
            ),
            (
                value!({foo: "bar", baz: "qux"}),
                vec![],
                false,
                Some(value!({foo: "bar", baz: "qux"})),
                Some(value!({})),
            ),
            (
                value!({foo: "bar", baz: "qux"}),
                vec![],
                true,
                Some(value!({foo: "bar", baz: "qux"})),
                Some(value!({})),
            ),
            (
                value!({foo: [0]}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                false,
                Some(value!(0)),
                Some(value!({foo: []})),
            ),
            (
                value!({foo: [0]}),
                vec![Field(Regular("foo".to_owned())), Index(0)],
                true,
                Some(value!(0)),
                Some(value!({})),
            ),
            (
                value!({foo: {"bar baz": [0]}, bar: "baz"}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                false,
                Some(value!(0)),
                Some(value!({foo: {"bar baz": []}, bar: "baz"})),
            ),
            (
                value!({foo: {"bar baz": [0]}, bar: "baz"}),
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                true,
                Some(value!(0)),
                Some(value!({bar: "baz"})),
            ),
        ];

        for (mut object, segments, compact, value, expect) in cases {
            let path = Path::new_unchecked(segments);

            assert_eq!(Object::remove(&mut object, &path, compact), Ok(value));
            assert_eq!(Object::get(&object, &Path::root()), Ok(expect));
        }
    }
}
