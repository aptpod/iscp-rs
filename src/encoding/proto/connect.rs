use crate::encoding::internal::autogen;
use crate::encoding::internal::autogen::ConnectResponse;
use crate::message as msg;

impl From<autogen::ConnectRequest> for msg::ConnectRequest {
    fn from(r: autogen::ConnectRequest) -> Self {
        let access_token = if let Some(extension_fields) = &r.extension_fields {
            let token = extension_fields.access_token.clone();
            Some(msg::AccessToken::from(token))
        } else {
            None
        };
        let project_uuid = if let Some(extension_fields) = &r.extension_fields {
            if let Some(intdash_extension_fields) = &extension_fields.intdash {
                if !intdash_extension_fields.project_uuid.is_empty() {
                    Some(intdash_extension_fields.project_uuid.clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        Self {
            request_id: r.request_id.into(),
            protocol_version: r.protocol_version,
            node_id: r.node_id,
            ping_interval: chrono::Duration::seconds(r.ping_interval.into()),
            ping_timeout: chrono::Duration::seconds(r.ping_timeout.into()),
            project_uuid,
            access_token,
        }
    }
}

impl From<autogen::ConnectRequest> for msg::Message {
    fn from(r: autogen::ConnectRequest) -> Self {
        let req = msg::ConnectRequest::from(r);
        msg::Message::from(req)
    }
}

impl From<msg::ConnectRequest> for autogen::ConnectRequest {
    fn from(r: msg::ConnectRequest) -> Self {
        let mut res = Self {
            request_id: r.request_id.value(),
            protocol_version: r.protocol_version,
            node_id: r.node_id,
            ping_interval: r.ping_interval.num_seconds() as u32,
            ping_timeout: r.ping_timeout.num_seconds() as u32,
            ..Default::default()
        };

        if let Some(access_token) = r.access_token {
            let ext = autogen::ConnectRequestExtensionFields {
                access_token: access_token.into(),
                intdash: None,
            };
            res.extension_fields = Some(ext);
        }

        if let Some(project_uuid) = r.project_uuid {
            let intdash_extension_fields = autogen::IntdashExtensionFields { project_uuid };
            if let Some(ext) = &mut res.extension_fields {
                ext.intdash = Some(intdash_extension_fields);
            } else {
                res.extension_fields = Some(autogen::ConnectRequestExtensionFields {
                    access_token: "".into(),
                    intdash: Some(intdash_extension_fields),
                });
            }
        }

        res
    }
}

impl From<autogen::ConnectResponse> for msg::ConnectResponse {
    fn from(r: autogen::ConnectResponse) -> Self {
        Self {
            request_id: r.request_id.into(),
            protocol_version: r.protocol_version,
            result_code: r.result_code.into(),
            result_string: r.result_string,
        }
    }
}

impl From<autogen::ConnectResponse> for msg::Message {
    fn from(r: autogen::ConnectResponse) -> Self {
        let req = msg::ConnectResponse::from(r);
        msg::Message::from(req)
    }
}

impl From<msg::ConnectResponse> for ConnectResponse {
    fn from(r: msg::ConnectResponse) -> Self {
        Self {
            request_id: r.request_id.value(),
            protocol_version: r.protocol_version,
            result_code: r.result_code.into(),
            result_string: r.result_string,
            ..Default::default()
        }
    }
}

impl From<autogen::Disconnect> for msg::Disconnect {
    fn from(r: autogen::Disconnect) -> Self {
        Self {
            result_code: r.result_code.into(),
            result_string: r.result_string,
        }
    }
}

impl From<autogen::Disconnect> for msg::Message {
    fn from(r: autogen::Disconnect) -> Self {
        let req = msg::Disconnect::from(r);
        msg::Message::from(req)
    }
}

impl From<msg::Disconnect> for autogen::Disconnect {
    fn from(r: msg::Disconnect) -> Self {
        Self {
            result_code: r.result_code.into(),
            result_string: r.result_string,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn connect_request() {
        use autogen::ConnectRequest;
        struct Case {
            msg: msg::ConnectRequest,
            proto: ConnectRequest,
        }
        impl Case {
            pub fn test(&self) {
                let p2m = msg::ConnectRequest::from(self.proto.clone());
                assert_eq!(p2m, self.msg.clone());
                let m2p = ConnectRequest::from(self.msg.clone());
                assert_eq!(m2p, self.proto.clone());
            }
        }
        let cases = vec![Case {
            msg: msg::ConnectRequest {
                request_id: 1.into(),
                protocol_version: "v1.0.0".to_string(),
                node_id: "2c2912e5-ce57-4453-a528-39850029744e".to_string(),
                ping_interval: chrono::Duration::seconds(10),
                ping_timeout: chrono::Duration::seconds(1),
                access_token: Some(msg::AccessToken::new("access_token")),
                project_uuid: None,
            },
            proto: ConnectRequest {
                request_id: 1,
                protocol_version: "v1.0.0".to_string(),
                node_id: "2c2912e5-ce57-4453-a528-39850029744e".to_string(),
                ping_interval: 10,
                ping_timeout: 1,
                extension_fields: Some(autogen::ConnectRequestExtensionFields {
                    access_token: "access_token".into(),
                    ..Default::default()
                }),
            },
        }];
        cases.into_iter().for_each(|case| {
            case.test();
        });
    }

    #[test]
    fn connect_response() {
        use autogen::ConnectResponse;
        let mut map: HashMap<String, Vec<u8>> = HashMap::new();
        map.insert("hoge".into(), vec![1, 2, 3, 4]);
        struct Case {
            msg: msg::ConnectResponse,
            proto: ConnectResponse,
        }
        impl Case {
            pub fn test(&self) {
                let p2m = msg::ConnectResponse::from(self.proto.clone());
                assert_eq!(p2m, self.msg.clone());
                let m2p = ConnectResponse::from(self.msg.clone());
                assert_eq!(m2p, self.proto.clone());
            }
        }
        let cases = vec![Case {
            msg: msg::ConnectResponse {
                request_id: 1.into(),
                protocol_version: "v1.0.0".to_string(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "ok".to_string(),
            },
            proto: ConnectResponse {
                request_id: 1,
                protocol_version: "v1.0.0".to_string(),
                result_code: autogen::ResultCode::Succeeded.into(),
                result_string: "ok".to_string(),
                ..Default::default()
            },
        }];
        cases.into_iter().for_each(|case| {
            case.test();
        });
    }
    #[test]
    fn disconnect() {
        use autogen::Disconnect;
        let mut map: HashMap<String, Vec<u8>> = HashMap::new();
        map.insert("hoge".into(), vec![1, 2, 3, 4]);
        struct Case {
            msg: msg::Disconnect,
            proto: Disconnect,
        }
        impl Case {
            pub fn test(&self) {
                let p2m = msg::Disconnect::from(self.proto.clone());
                assert_eq!(p2m, self.msg.clone());
                let m2p = Disconnect::from(self.msg.clone());
                assert_eq!(m2p, self.proto.clone());
            }
        }
        let cases = vec![Case {
            msg: msg::Disconnect {
                result_code: msg::ResultCode::Succeeded,
                result_string: "ok".to_string(),
            },
            proto: Disconnect {
                result_code: autogen::ResultCode::Succeeded.into(),
                result_string: "ok".to_string(),
                ..Default::default()
            },
        }];
        cases.into_iter().for_each(|case| {
            case.test();
        });
    }
}
