//! Serde helpers for Effect Schema encodings that plain derives do not cover.
//!
//! - [`open_enum!`]: string literal unions that grow over time. Unknown strings decode into an
//!   `Other(String)` variant instead of failing.
//! - [`id!`]: branded string ids (`ThreadId`, `ProjectId`, ...) as distinct newtypes.
//! - [`forward_compatible`]: `ForwardCompatibleArray(X)`, where undecodable elements are dropped.

use serde::{Deserialize, Deserializer, de::DeserializeOwned};

/// Declares a string enum with an `Other(String)` catch-all.
///
/// The wire form is the literal string. Use it for every literal union the server may extend
/// (statuses, kinds, modes). Clients should only construct the named variants.
#[macro_export]
macro_rules! open_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $value:literal, )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant, )*
            /// A value this client does not know yet.
            Other(String),
        }

        impl $name {
            /// The wire string.
            pub fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $value, )*
                    Self::Other(value) => value,
                }
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                match value {
                    $( $value => Self::$variant, )*
                    other => Self::Other(other.to_owned()),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                deserializer.deserialize_str($crate::schema::StrVisitor).map(|s| Self::from(&*s))
            }
        }
    };
}

/// Declares a branded string id. Client-created ids use [`random`](ThreadId::random) (UUID v4,
/// what upstream clients send).
#[macro_export]
macro_rules! id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// A fresh UUID v4 id.
            pub fn random() -> Self {
                Self($crate::schema::uuid_v4())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }
    };
}

#[doc(hidden)]
pub fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Visitor that accepts borrowed or owned strings. Used by [`open_enum!`].
#[doc(hidden)]
pub struct StrVisitor;

impl<'de> serde::de::Visitor<'de> for StrVisitor {
    type Value = std::borrow::Cow<'de, str>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a string")
    }

    fn visit_borrowed_str<E: serde::de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
        Ok(v.into())
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(v.to_owned().into())
    }

    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(v.into())
    }
}

/// `deserialize_with` for `ForwardCompatibleArray(X)` fields: elements that fail to decode are
/// dropped instead of failing the whole document (`packages/contracts/src/baseSchemas.ts:110`).
pub fn forward_compatible<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}

/// `deserialize_with` for `ForwardCompatibleArray(X) | null` fields that keep `null` distinct
/// from an empty list. Pair with `#[serde(default)]`.
pub fn forward_compatible_option<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let values = Option::<Vec<serde_json::Value>>::deserialize(deserializer)?;
    Ok(values.map(|values| {
        values
            .into_iter()
            .filter_map(|value| serde_json::from_value(value).ok())
            .collect()
    }))
}

/// `deserialize_with` for nullable `ForwardCompatibleArray` fields (`null` or missing is empty).
pub fn forward_compatible_or_null<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let values = Option::<Vec<serde_json::Value>>::deserialize(deserializer)?;
    Ok(values
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}

/// `deserialize_with` for `optional(NullOr(X))` patch fields where absent, `null`, and a value
/// mean three different things: no change (`None`), clear (`Some(None)`), set (`Some(Some(v))`).
/// Pair with `#[serde(default)]` so a missing key stays `None`.
pub fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// `deserialize_with` for optional fields inside forward-compatible rows (upstream
/// `ForwardCompatibleOptional`): a value that does not decode becomes `None` instead of failing
/// (and dropping) the whole row. Pair with `#[serde(default)]`.
pub fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

/// `deserialize_with` for `Array(X) | null` fields the client treats as "empty when null".
pub fn null_as_empty<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}
