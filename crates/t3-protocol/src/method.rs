//! Typed RPC method descriptors.
//!
//! Each server RPC is a zero-sized type implementing [`Unary`] or [`Stream`]. The client
//! is generic over these, so payload/response mismatches are compile errors:
//!
//! ```ignore
//! let config = client.request::<ServerGetConfig>(&Empty {}).await?;
//! let mut shell = client.subscribe::<SubscribeShell>(&Empty {});
//! ```

use serde::{Serialize, de::DeserializeOwned};

/// A request/response RPC.
pub trait Unary {
    /// Wire tag, e.g. `"server.getConfig"`.
    const TAG: &'static str;
    type Payload: Serialize;
    type Success: DeserializeOwned + Send + 'static;
    type Error: DeserializeOwned + Send + 'static;
}

/// A streaming RPC (subscriptions and long-running commands).
pub trait Stream {
    const TAG: &'static str;
    type Payload: Serialize;
    type Item: DeserializeOwned + Send + 'static;
    type Error: DeserializeOwned + Send + 'static;
}

/// Declares a unary method type.
#[macro_export]
macro_rules! unary {
    ($(#[$meta:meta])* $name:ident, $tag:literal, $payload:ty => $success:ty, $error:ty) => {
        $(#[$meta])*
        pub enum $name {}
        impl $crate::method::Unary for $name {
            const TAG: &'static str = $tag;
            type Payload = $payload;
            type Success = $success;
            type Error = $error;
        }
    };
}

/// Declares a streaming method type.
#[macro_export]
macro_rules! stream {
    ($(#[$meta:meta])* $name:ident, $tag:literal, $payload:ty => $item:ty, $error:ty) => {
        $(#[$meta])*
        pub enum $name {}
        impl $crate::method::Stream for $name {
            const TAG: &'static str = $tag;
            type Payload = $payload;
            type Item = $item;
            type Error = $error;
        }
    };
}
