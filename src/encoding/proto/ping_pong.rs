use crate::encoding::internal::autogen::{Ping, Pong};
use crate::message as msg;

impl From<msg::Ping> for Ping {
    fn from(p: msg::Ping) -> Self {
        Self {
            request_id: p.request_id.value(),
            ..Default::default()
        }
    }
}

impl From<Ping> for msg::Message {
    fn from(p: Ping) -> Self {
        let res: msg::Ping = p.into();
        res.into()
    }
}

impl From<Ping> for msg::Ping {
    fn from(p: Ping) -> Self {
        Self {
            request_id: p.request_id.into(),
        }
    }
}

impl From<msg::Pong> for Pong {
    fn from(p: msg::Pong) -> Self {
        Self {
            request_id: p.request_id.value(),
            ..Default::default()
        }
    }
}

impl From<Pong> for msg::Message {
    fn from(p: Pong) -> Self {
        let res: msg::Pong = p.into();
        res.into()
    }
}

impl From<Pong> for msg::Pong {
    fn from(p: Pong) -> Self {
        Self {
            request_id: p.request_id.into(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_proto_from_to_ping() {
        let proto = Ping {
            request_id: 100,
            ..Default::default()
        };
        let msg = msg::Ping {
            request_id: 100.into(),
        };
        assert_eq!(msg, msg::Ping::from(proto.clone()));
        assert_eq!(Ping::from(msg), proto);
    }

    #[test]
    fn test_proto_from_to_pong() {
        let proto = Pong {
            request_id: 100,
            ..Default::default()
        };
        let msg = msg::Pong {
            request_id: 100.into(),
        };

        assert_eq!(msg::Pong::from(proto.clone()), msg);
        assert_eq!(proto, Pong::from(msg));
    }
}
