//! エンコード定義

use bytes::BytesMut;

use crate::message::Message;

#[derive(Debug)]
#[non_exhaustive]
pub enum Name {
    Protobuf,
}

pub struct EncodingError {
    inner: Box<dyn std::error::Error + Send>,
}

impl std::fmt::Debug for EncodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EncodingError").field(&self.inner).finish()
    }
}

impl std::fmt::Display for EncodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "encoding error: {}", self.inner)
    }
}

impl std::error::Error for EncodingError {}

impl EncodingError {
    pub fn new<E: std::error::Error + Send + 'static>(err: E) -> Self {
        Self {
            inner: Box::new(err),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EncodingBuilder {}

impl EncodingBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(self) -> Encoding {
        Encoding {
            encoder: Encoder {},
            decoder: Decoder {},
        }
    }
}

#[derive(Clone, Debug)]
pub struct Encoding {
    pub(crate) encoder: Encoder,
    pub(crate) decoder: Decoder,
}

impl Encoding {
    pub fn name(&self) -> Name {
        Name::Protobuf
    }
}

#[derive(Debug)]
pub struct Encoder {}

impl Clone for Encoder {
    fn clone(&self) -> Self {
        Self {}
    }
}

impl Encoder {
    pub fn encode_to(&mut self, buf: &mut BytesMut, msg: &Message) -> Result<(), EncodingError> {
        prost::Message::encode(msg, buf).map_err(EncodingError::new)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Decoder {}

impl Clone for Decoder {
    fn clone(&self) -> Self {
        Self {}
    }
}

impl Decoder {
    pub fn decode_from(&mut self, data: &[u8]) -> Result<Message, EncodingError> {
        prost::Message::decode(data).map_err(EncodingError::new)
    }
}
