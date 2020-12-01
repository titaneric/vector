mod kind;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::convert::{TryFrom, TryInto};
use std::fmt;

pub use kind::Kind;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Map(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Timestamp(DateTime<Utc>),
    Null,
}

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error(r#"expected "{0}", got "{1}""#)]
    Expected(Kind, Kind),

    #[error(r#"unable to coerce "{0}" into "{1}""#)]
    Coerce(Kind, Kind),

    #[error("unable to calculate remainder of values type {0} and {1}")]
    Rem(Kind, Kind),

    #[error("unable to multiply value type {0} by {1}")]
    Mul(Kind, Kind),

    #[error("unable to divide value type {0} by {1}")]
    Div(Kind, Kind),

    #[error("unable to add value type {1} to {0}")]
    Add(Kind, Kind),

    #[error("unable to subtract value type {1} from {0}")]
    Sub(Kind, Kind),

    #[error("unable to OR value type {0} with {1}")]
    Or(Kind, Kind),

    #[error("unable to AND value type {0} with {1}")]
    And(Kind, Kind),

    #[error("unable to compare {0} > {1}")]
    Gt(Kind, Kind),

    #[error("unable to compare {0} >= {1}")]
    Ge(Kind, Kind),

    #[error("unable to compare {0} < {1}")]
    Lt(Kind, Kind),

    #[error("unable to compare {0} <= {1}")]
    Le(Kind, Kind),
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Integer(v as i64)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Integer(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<Bytes> for Value {
    fn from(v: Bytes) -> Self {
        Value::Bytes(v)
    }
}

impl From<Cow<'_, str>> for Value {
    fn from(v: Cow<'_, str>) -> Self {
        v.as_ref().into()
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        v.as_slice().into()
    }
}

impl From<&[u8]> for Value {
    fn from(v: &[u8]) -> Self {
        Value::Bytes(Bytes::copy_from_slice(v))
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Bytes(v.into())
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Boolean(v)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect::<Vec<_>>())
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Bytes(Bytes::copy_from_slice(v.as_bytes()))
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Null
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Map(value)
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(v: DateTime<Utc>) -> Self {
        Value::Timestamp(v)
    }
}

impl TryFrom<&Value> for f64 {
    type Error = Error;

    fn try_from(value: &Value) -> std::result::Result<Self, Self::Error> {
        match value {
            Value::Integer(v) => Ok(*v as f64),
            Value::Float(v) => Ok(*v),
            _ => Err(Error::Coerce(value.kind(), Kind::Float)),
        }
    }
}

impl TryFrom<&Value> for i64 {
    type Error = Error;

    fn try_from(value: &Value) -> std::result::Result<Self, Self::Error> {
        match value {
            Value::Integer(v) => Ok(*v),
            Value::Float(v) => Ok(*v as i64),
            _ => Err(Error::Coerce(value.kind(), Kind::Integer)),
        }
    }
}

impl TryFrom<&Value> for String {
    type Error = Error;

    fn try_from(value: &Value) -> std::result::Result<Self, Self::Error> {
        use Value::*;

        match value {
            Bytes(v) => Ok(String::from_utf8_lossy(&v).into_owned()),
            Integer(v) => Ok(format!("{}", v)),
            Float(v) => Ok(format!("{}", v)),
            Boolean(v) => Ok(format!("{}", v)),
            Null => Ok("".to_owned()),
            _ => Err(Error::Coerce(value.kind(), Kind::Bytes)),
        }
    }
}

impl TryFrom<Value> for String {
    type Error = Error;

    fn try_from(value: Value) -> std::result::Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<Value> for i64 {
    type Error = Error;

    fn try_from(value: Value) -> std::result::Result<Self, Self::Error> {
        (&value).try_into()
    }
}

macro_rules! value_impl {
    ($(($func:expr, $variant:expr, $ret:ty)),+ $(,)*) => {
        impl Value {
            $(paste::paste! {
            pub fn [<as_ $func>](&self) -> Option<&$ret> {
                match self {
                    Value::$variant(v) => Some(v),
                    _ => None,
                }
            }

            pub fn [<as_ $func _mut>](&mut self) -> Option<&mut $ret> {
                match self {
                    Value::$variant(v) => Some(v),
                    _ => None,
                }
            }

            pub fn [<try_ $func>](self) -> Result<$ret, Error> {
                match self {
                    Value::$variant(v) => Ok(v),
                    v => Err(Error::Expected(Kind::$variant, v.kind())),
                }
            }

            pub fn [<unwrap_ $func>](self) -> $ret {
                self.[<try_ $func>]().expect(stringify!($func))
            }
            })+

            pub fn as_null(&self) -> Option<()> {
                match self {
                    Value::Null => Some(()),
                    _ => None,
                }
            }

            pub fn try_null(self) -> Result<(), Error> {
                match self {
                    Value::Null => Ok(()),
                    v => Err(Error::Expected(Kind::Null, v.kind())),
                }
            }

            pub fn unwrap_null(self) -> () {
                self.try_null().expect("null")
            }
        }
    };
}

value_impl! {
    (bytes, Bytes, Bytes),
    (integer, Integer, i64),
    (float, Float, f64),
    (boolean, Boolean, bool),
    (map, Map, BTreeMap<String, Value>),
    (array, Array, Vec<Value>),
    (timestamp, Timestamp, DateTime<Utc>),
    // manually implemented due to no variant value
    // (null, Null, ()),
}

impl Value {
    pub fn kind(&self) -> Kind {
        self.into()
    }

    /// Similar to [`std::ops::Mul`], but fallible (e.g. `TryMul`).
    pub fn try_mul(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Mul(self.kind(), rhs.kind());

        let value = match &self {
            Value::Bytes(lhv) => lhv
                .repeat(i64::try_from(&rhs).map_err(|_| err())? as usize)
                .into(),
            Value::Integer(lhv) => (lhv * i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv * f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Div`], but fallible (e.g. `TryDiv`).
    pub fn try_div(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Div(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv / i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv / f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Add`], but fallible (e.g. `TryAdd`).
    pub fn try_add(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Add(self.kind(), rhs.kind());

        let value = match &self {
            Value::Bytes(lhv) => format!(
                "{}{}",
                String::from_utf8_lossy(&lhv),
                String::try_from(&rhs).map_err(|_| err())?
            )
            .into(),
            Value::Integer(lhv) => (lhv + i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv + f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Sub`], but fallible (e.g. `TrySub`).
    pub fn try_sub(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Sub(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv - i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv - f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// "OR" (`||`) two values types.
    ///
    /// A lhs value of `Null` or a `false` delegates to the rhs value,
    /// everything else delegates to `lhs`.
    pub fn or(self, rhs: Self) -> Self {
        match self {
            Value::Null => rhs,
            Value::Boolean(lhv) if !lhv => rhs,
            value => value,
        }
    }

    /// Try to "AND" (`&&`) two values types.
    ///
    /// A lhs or rhs value of `Null` returns `false`.
    ///
    /// TODO: this should maybe work similar to `OR`, in that it supports any
    /// rhs value kind, to support: `true && "foo"` to resolve to "foo".
    pub fn try_and(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Or(self.kind(), rhs.kind());

        let value = match self {
            Value::Null => false.into(),
            Value::Boolean(lhv) => match rhs {
                Value::Null => false.into(),
                Value::Boolean(rhv) => (lhv && rhv).into(),
                _ => return Err(err()),
            },
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::ops::Rem`], but fallible (e.g. `TryRem`).
    pub fn try_rem(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Rem(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv % i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv % f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_gt(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Rem(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv > i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv > f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_ge(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv >= i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv >= f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_lt(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv < i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv < f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Ord`], but fallible (e.g. `TryOrd`).
    pub fn try_le(self, rhs: Self) -> Result<Self, Error> {
        let err = || Error::Ge(self.kind(), rhs.kind());

        let value = match self {
            Value::Integer(lhv) => (lhv <= i64::try_from(&rhs).map_err(|_| err())?).into(),
            Value::Float(lhv) => (lhv <= f64::try_from(&rhs).map_err(|_| err())?).into(),
            _ => return Err(err()),
        };

        Ok(value)
    }

    /// Similar to [`std::cmp::Eq`], but does a lossless comparison for integers
    /// and floats.
    pub fn eq_lossy(&self, rhs: &Self) -> bool {
        use Value::*;

        match self {
            // FIXME: when cmoparing ints to floats, always change the int to
            // float, not the other way around
            //
            // Do the same for multiplication, etc.
            Integer(lhv) => i64::try_from(rhs).map(|rhv| *lhv == rhv).unwrap_or(false),
            Float(lhv) => f64::try_from(rhs).map(|rhv| *lhv == rhv).unwrap_or(false),
            _ => self == rhs,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bytes(val) => write!(f, "{}", String::from_utf8_lossy(val)),
            Value::Integer(val) => write!(f, "{}", val),
            Value::Float(val) => write!(f, "{}", val),
            Value::Boolean(val) => write!(f, "{}", val),
            Value::Map(map) => {
                let joined = map
                    .iter()
                    .map(|(key, val)| format!("{}: {}", key, val))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{{ {} }}", joined)
            }
            Value::Array(array) => {
                let joined = array
                    .iter()
                    .map(|val| format!("{}", val))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "[{}]", joined)
            }
            Value::Timestamp(val) => write!(f, "{}", val.to_string()),
            Value::Null => write!(f, "Null"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;

    #[test]
    fn test_display() {
        let string = format!("{}", Value::from("Sausage 🌭"));
        assert_eq!("Sausage 🌭", string);

        let int = format!("{}", Value::from(42));
        assert_eq!("42", int);

        let float = format!("{}", Value::from(42.5));
        assert_eq!("42.5", float);

        let boolean = format!("{}", Value::from(true));
        assert_eq!("true", boolean);

        let mut map = BTreeMap::new();
        map.insert("field".to_string(), Value::from(1));
        let map = format!("{}", Value::Map(map));
        assert_eq!("{ field: 1 }", map);

        let array = format!("{}", Value::from(vec![1, 2, 3]));
        assert_eq!("[1, 2, 3]", array);

        let timestamp = format!("{}", Value::from(Utc.ymd(2020, 10, 21).and_hms(16, 20, 13)));
        assert_eq!("2020-10-21 16:20:13 UTC", timestamp);

        let null = format!("{}", Value::Null);
        assert_eq!("Null", null);
    }
}
