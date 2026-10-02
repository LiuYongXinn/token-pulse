use serde_json::{Value, json};
use token_pulse_core::{
    error::ErrorCode,
    quota::{
        AccountAvailability, QuotaBook, QuotaUpdate, account_availability, parse_quota_read,
        parse_quota_update,
    },
};

/// There is intentionally no caller-supplied method, parameters, token or shell command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountRequest {
    ReadAccount,
    ReadLimits,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequestKind {
    Initialize,
    Account(AccountRequest),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcToken {
    pub connection_epoch: String,
    pub request_id: String,
}
#[derive(Debug)]
pub enum RpcReply {
    Initialized,
    Account(AccountAvailability),
    Limits(QuotaBook),
}
#[derive(Debug)]
pub enum ProtocolEvent {
    Reply {
        token: RpcToken,
        result: Result<RpcReply, ErrorCode>,
    },
    TimedOut {
        token: RpcToken,
    },
    AccountChanged,
    LimitsUpdated(QuotaUpdate),
}

pub(crate) fn request(kind: RequestKind, id: &str) -> Value {
    match kind {
        RequestKind::Initialize => json!({"id": id, "method":"initialize", "params": {
            "clientInfo":{"name":"token_pulse", "title":"TokenPulse", "version":env!("CARGO_PKG_VERSION")},
            "capabilities":{"experimentalApi":false}
        }}),
        RequestKind::Account(AccountRequest::ReadAccount) => {
            json!({"id":id,"method":"account/read","params":{"refreshToken":false}})
        }
        RequestKind::Account(AccountRequest::ReadLimits) => {
            json!({"id":id,"method":"account/rateLimits/read"})
        }
    }
}
pub(crate) fn initialized() -> Value {
    json!({"method":"initialized","params":{}})
}

pub(crate) fn reply(kind: RequestKind, value: &Value) -> Result<RpcReply, ErrorCode> {
    let object = value.as_object().ok_or(ErrorCode::QuotaProtocolError)?;
    if object.contains_key("result") == object.contains_key("error") {
        return Err(ErrorCode::QuotaProtocolError);
    }
    if let Some(error) = object.get("error") {
        // Do not retain or classify by server-provided prose, data or auth details.
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .ok_or(ErrorCode::QuotaProtocolError)?;
        return Err(if code == -32601 {
            ErrorCode::QuotaUnsupported
        } else {
            ErrorCode::QuotaServiceUnavailable
        });
    }
    let result = &object["result"];
    match kind {
        RequestKind::Initialize => {
            let user_agent = result
                .get("userAgent")
                .and_then(Value::as_str)
                .ok_or(ErrorCode::QuotaProtocolError)?;
            if user_agent.is_empty()
                || user_agent.len() > 4096
                || user_agent.chars().any(char::is_control)
            {
                return Err(ErrorCode::QuotaProtocolError);
            }
            // platform, home and other response fields are neither cached nor displayed.
            Ok(RpcReply::Initialized)
        }
        RequestKind::Account(AccountRequest::ReadAccount) => {
            account_availability(result).map(RpcReply::Account)
        }
        RequestKind::Account(AccountRequest::ReadLimits) => {
            parse_quota_read(result).map(RpcReply::Limits)
        }
    }
}
pub(crate) fn notification(
    method: &str,
    params: Option<&Value>,
) -> Result<Option<ProtocolEvent>, ErrorCode> {
    match method {
        "account/updated" => Ok(Some(ProtocolEvent::AccountChanged)),
        "account/rateLimits/updated" => {
            parse_quota_update(params.ok_or(ErrorCode::QuotaProtocolError)?)
                .map(|update| Some(ProtocolEvent::LimitsUpdated(update)))
        }
        // Including login / model / conversation / tool notifications: never retained.
        _ => Ok(None),
    }
}
pub(crate) fn reject_server_request(id: &Value) -> Result<Value, ErrorCode> {
    let valid = match id {
        Value::String(s) => !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control),
        Value::Number(n) => n.as_i64().is_some() || n.as_u64().is_some(),
        _ => false,
    };
    if !valid {
        return Err(ErrorCode::QuotaProtocolError);
    }
    Ok(json!({"id":id,"error":{"code":-32601,"message":"Unsupported method"}}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn methods_and_fields_are_fixed_and_replies_discard_sensitive_extras() {
        assert_eq!(
            request(RequestKind::Account(AccountRequest::ReadAccount), "id"),
            json!({"id":"id","method":"account/read","params":{"refreshToken":false}})
        );
        assert_eq!(
            request(RequestKind::Account(AccountRequest::ReadLimits), "id"),
            json!({"id":"id","method":"account/rateLimits/read"})
        );
        let init = request(RequestKind::Initialize, "i");
        assert_eq!(
            init["params"]["capabilities"],
            json!({"experimentalApi":false})
        );
        assert!(matches!(
            reply(
                RequestKind::Initialize,
                &json!({"result":{"userAgent":"synthetic", "codexHome":"SECRET"}})
            )
            .unwrap(),
            RpcReply::Initialized
        ));
        let account = reply(RequestKind::Account(AccountRequest::ReadAccount), &json!({"result":{"requiresOpenaiAuth":true,"account":{"type":"chatgpt","email":"SECRET", "tokens":"SECRET"}}})).unwrap();
        assert_eq!(format!("{account:?}"), "Account(QuotaEligible)");
    }
    #[test]
    fn response_shapes_and_server_requests_are_controlled() {
        for value in [
            json!({}),
            json!({"result":{},"error":{"code":-32601}}),
            json!({"error":{"message":"SECRET"}}),
            json!({"result":{"userAgent":null}}),
        ] {
            assert_eq!(
                reply(RequestKind::Initialize, &value).unwrap_err(),
                ErrorCode::QuotaProtocolError
            );
        }
        assert_eq!(
            reply(
                RequestKind::Initialize,
                &json!({"error":{"code":-32601,"message":"SECRET", "data":{"token":"SECRET"}}})
            )
            .unwrap_err(),
            ErrorCode::QuotaUnsupported
        );
        assert_eq!(
            reply(
                RequestKind::Initialize,
                &json!({"error":{"code":401,"message":"SECRET"}})
            )
            .unwrap_err(),
            ErrorCode::QuotaServiceUnavailable
        );
        assert_eq!(
            reject_server_request(&json!(19)).unwrap(),
            json!({"id":19,"error":{"code":-32601,"message":"Unsupported method"}})
        );
        for id in [Value::Null, json!({}), json!(1.5), json!("\n")] {
            assert_eq!(
                reject_server_request(&id),
                Err(ErrorCode::QuotaProtocolError)
            );
        }
        assert!(
            notification("thread/sensitive", Some(&json!({"text":"SECRET"})))
                .unwrap()
                .is_none()
        );
    }
}
