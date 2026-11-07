//! Reading the fields of an agent's records leniently: the formats are internal to the agents and
//! change between versions, so one field of an unexpected type costs that field, not the record.

use std::fmt;
use std::marker::PhantomData;

use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};

/// Reads a small field as `T`, or as `None` when it holds another type, so that one changed field
/// does not cost the whole record.
pub(crate) fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Field<T> {
        Value(T),
        Other(IgnoredAny),
    }
    Ok(match Field::<T>::deserialize(deserializer)? {
        Field::Value(value) => Some(value),
        Field::Other(_) => None,
    })
}

/// Reads a list, keeping the elements that read as `T`; anything other than a list reads as empty.
pub(crate) fn lenient_list<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(lenient::<D, Vec<Lenient<T>>>(deserializer)?
        .unwrap_or_default()
        .into_iter()
        .filter_map(|element| element.0)
        .collect())
}

/// One element of a list read by [`lenient_list`].
struct Lenient<T>(Option<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Lenient<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        lenient(deserializer).map(Lenient)
    }
}

/// Reads an object as `T`, or as `None` when the field holds something else.
///
/// Unlike [`lenient`], it reads the object in place instead of buffering it first, because a
/// record's message holds the whole response.
pub(crate) fn object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = Option<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("any value")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map)).map(Some)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok(None)
        }

        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(ObjectVisitor(PhantomData))
}
