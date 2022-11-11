//! iSCPのメッセージをエンコードする方法を定義したモジュールです。

pub mod internal;

pub mod json;
pub mod proto;

use crate::error::Result;

/// iSCP のエンコード層を抽象化したインターフェースです。
pub trait Encoder: Send + Sync {
    /// iSCP のメッセージをバイナリへエンコードします。
    fn encode(&self, msg: crate::message::Message) -> Result<Vec<u8>>;
    /// 与えられたバイナリを、 iSCP のメッセージへデコードします。
    fn decode(&self, bin: &[u8]) -> Result<crate::message::Message>;
}

pub type BoxedEncoder = Box<dyn Encoder>;
impl<E: Encoder + ?Sized> Encoder for Box<E> {
    fn encode(&self, msg: crate::message::Message) -> Result<Vec<u8>> {
        (**self).encode(msg)
    }
    fn decode(&self, bin: &[u8]) -> Result<crate::message::Message> {
        (**self).decode(bin)
    }
}

/// エンコーディングの形式を表します。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    /// Protocol Buffers 形式のエンコーディングを表します。
    Proto,
    /// JSON 形式のエンコーディングを表します。
    Json,
}

impl Encoding {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn generate(&self) -> Result<BoxedEncoder> {
        match self {
            Self::Proto => Ok(Box::new(proto::Encoder)),
            Self::Json => Ok(Box::new(json::Encoder)),
        }
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self::Proto
    }
}
