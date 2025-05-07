use bytes::BufMut;

use crate::encoding;
use crate::transport::TransportError;

/// エンコード方法
#[derive(Clone, Debug, serde::Serialize)]
#[non_exhaustive]
pub enum EncodingName {
    #[serde(rename = "proto")]
    Protobuf,
}

impl From<encoding::Name> for EncodingName {
    fn from(name: encoding::Name) -> Self {
        match name {
            encoding::Name::Protobuf => EncodingName::Protobuf,
        }
    }
}

/// 圧縮タイプ
#[derive(Clone, Debug, serde::Serialize)]
pub enum CompressionType {
    #[serde(rename = "per-message")]
    PerMessage,
    #[serde(rename = "context-takeover")]
    ContextTakeover,
}

/// 接続開始時のパラメータ
#[derive(Clone, Debug, serde::Serialize)]
#[non_exhaustive]
pub struct NegotiationParams {
    #[serde(rename = "enc")]
    pub encoding_name: EncodingName,
    #[serde(rename = "comp")]
    pub compression_type: Option<CompressionType>,
    #[serde(rename = "clevel")]
    pub compression_level: Option<i8>,
    #[serde(rename = "cwinbits")]
    pub compression_window_bits: Option<u8>,
}

impl NegotiationParams {
    pub(crate) fn to_uri_query_string(&self) -> Result<String, TransportError> {
        serde_qs::to_string(&self).map_err(TransportError::new)
    }

    pub(crate) fn to_bytes(&self) -> Result<Vec<u8>, TransportError> {
        use serde_json::Value;

        let mut buf = Vec::new();

        let value = serde_json::to_value(self).unwrap();
        let map = value.as_object().unwrap();
        for (k, v) in map.iter() {
            let v = match v {
                Value::Null => continue,
                Value::String(s) => s.clone(),
                _ => v.to_string(),
            };

            buf.put_u16(
                k.len()
                    .try_into()
                    .map_err(|_| TransportError::from_msg("too long key for negotiation"))?,
            );
            buf.put(k.as_bytes());

            buf.put_u16(
                v.len()
                    .try_into()
                    .map_err(|_| TransportError::from_msg("too long value for negotiation"))?,
            );
            buf.put(v.as_bytes());
        }

        Ok(buf)
    }
}
