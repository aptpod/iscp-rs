#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
mod internal;

pub mod encoding;
mod error;
mod iscp;
pub mod message;
mod token_source;
pub mod transport;
pub mod wire;

pub use crate::error::*;
pub use crate::iscp::*;
pub use crate::token_source::*;

/// iSCP protocol version.
pub const ISCP_VERSION: &str = "2.0.0";
