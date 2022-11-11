use crate::error::Error;
use bytes::BufMut;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct NegotiationQuery {
    pub enc: String,
}

impl Default for NegotiationQuery {
    fn default() -> Self {
        Self {
            enc: "proto".into(),
        }
    }
}

impl NegotiationQuery {
    pub fn query_string(&self) -> Result<String, Error> {
        serde_qs::to_string(self).map_err(Error::invalid_value)
    }

    pub fn bytes(&self) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();

        let enc = self.enc.as_bytes();
        buf.put_u16(3);
        buf.put(b"enc".as_slice());
        buf.put_u16(
            enc.len()
                .try_into()
                .map_err(|_| Error::invalid_value("too long \"enc\" for negotiation"))?,
        );
        buf.put(enc);

        Ok(buf)
    }
}
