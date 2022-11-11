//! JSON フォーマットを使用したエンコーダーを提供するモジュールです。

use super::internal::autogen;
use crate::error::{Error, Result};
use crate::message as msg;

/// JSON フォーマット用エンコーダーです。
#[derive(Clone, Copy, Debug)]
pub struct Encoder;

impl Encoder {
    fn encode(&self, msg: msg::Message) -> Result<Vec<u8>> {
        let res = autogen::Message::from(msg);
        let res = serde_json::to_vec(&res).map_err(Error::encode)?;
        Ok(res)
    }
    fn decode(&self, bin: &[u8]) -> Result<msg::Message> {
        let input = String::from_utf8(bin.to_vec()).map_err(Error::malformed_message)?;
        let proto: autogen::Message =
            serde_json::from_str(input.as_str()).map_err(Error::malformed_message)?;
        proto.try_into()
    }
}

impl super::Encoder for Encoder {
    fn encode(&self, msg: msg::Message) -> Result<Vec<u8>> {
        self.encode(msg)
    }
    fn decode(&self, bin: &[u8]) -> Result<crate::message::Message> {
        self.decode(bin)
    }
}

#[allow(clippy::vec_init_then_push)]
#[cfg(test)]
mod test {

    use super::*;
    #[test]
    fn serde() {
        struct Case {
            msg: msg::Message,
            // proto: Message,
        }
        impl Case {
            pub fn test(&self) {
                let encoded = Encoder.encode(self.msg.clone()).unwrap();
                let decoded = Encoder.decode(&encoded).unwrap();
                assert_eq!(decoded, self.msg);
            }
        }
        let mut cases = Vec::new();
        cases.push(Case {
            msg: msg::ConnectRequest {
                request_id: 1.into(),
                protocol_version: "v1.0.0".to_string(),
                node_id: "2dda7df9-cfd8-4aba-a577-f05dec10b1d9".to_string(),
                project_uuid: None,
                ping_interval: chrono::Duration::seconds(10),
                ping_timeout: chrono::Duration::seconds(1),
                access_token: Some(msg::AccessToken::new("access_token")),
            }
            .into(),
        });
        cases.into_iter().for_each(|case| {
            case.test();
        });
    }
}
