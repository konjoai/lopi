//! Minimal JSON-RPC 2.0 envelope types for the MCP stdio protocol.
//!
//! MCP frames each message as a single line of JSON-RPC 2.0 over the server's
//! stdin/stdout. This module models the three message shapes lopi needs —
//! [`Request`], [`Notification`], [`Response`] — plus an [`IdGen`] for
//! monotonic request ids and [`encode_line`] for the newline framing. It is
//! transport-agnostic and pure, so the wire format is fully unit-testable
//! without spawning a process.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicI64, Ordering};

/// The JSON-RPC protocol version string MCP speaks.
pub const JSONRPC_VERSION: &str = "2.0";

/// A JSON-RPC request: a method call carrying an `id` that awaits a [`Response`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Request {
    /// Always [`JSONRPC_VERSION`].
    pub jsonrpc: String,
    /// Correlates this request with its response.
    pub id: i64,
    /// The method name (e.g. `"tools/list"`).
    pub method: String,
    /// Method parameters, omitted from the wire when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    /// Build a request with `id`, `method`, and optional `params`.
    #[must_use]
    pub fn new(id: i64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC notification: a method call with no `id`, expecting no response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Notification {
    /// Always [`JSONRPC_VERSION`].
    pub jsonrpc: String,
    /// The method name (e.g. `"notifications/initialized"`).
    pub method: String,
    /// Method parameters, omitted from the wire when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Notification {
    /// Build a notification with `method` and optional `params`.
    #[must_use]
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC response: a `result` **or** an `error`, keyed to a request `id`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Response {
    /// Always [`JSONRPC_VERSION`].
    pub jsonrpc: String,
    /// The id of the request this answers, or `null` per JSON-RPC 2.0 when
    /// the server couldn't determine the request's id at all (e.g. the
    /// request itself failed to parse) — such a response can't be
    /// correlated to any specific in-flight call by id.
    pub id: Option<i64>,
    /// The success payload, present iff `error` is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// The failure object, present iff `result` is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// Collapse the response into a `Result`: the `result` value (or `Null`) on
    /// success, or the server's [`RpcError`] on failure.
    ///
    /// # Errors
    /// Returns the response's `error` object when the call failed.
    pub fn into_result(self) -> Result<Value, RpcError> {
        match self.error {
            Some(e) => Err(e),
            None => Ok(self.result.unwrap_or(Value::Null)),
        }
    }
}

/// A JSON-RPC error object (the `error` member of a failed [`Response`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, thiserror::Error)]
#[error("JSON-RPC error {code}: {message}")]
pub struct RpcError {
    /// Numeric error code (JSON-RPC reserved range or server-defined).
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional structured error data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A monotonic request-id source for one client connection.
#[derive(Debug, Default)]
pub struct IdGen(AtomicI64);

impl IdGen {
    /// The next id in sequence, starting at 1.
    pub fn next(&self) -> i64 {
        self.0.fetch_add(1, Ordering::Relaxed) + 1
    }
}

/// Serialize a JSON-RPC message to a single newline-terminated line — the MCP
/// stdio framing (one JSON object per line).
///
/// # Errors
/// Returns `Err` if the message cannot be serialized to JSON.
pub fn encode_line<T: Serialize>(msg: &T) -> serde_json::Result<String> {
    let mut s = serde_json::to_string(msg)?;
    s.push('\n');
    Ok(s)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn request_serializes_with_envelope_and_omits_absent_params() {
        let req = Request::new(7, "tools/list", None);
        let v: Value = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 7);
        assert_eq!(v["method"], "tools/list");
        assert!(v.get("params").is_none(), "absent params omitted");
    }

    #[test]
    fn request_round_trips_with_params() {
        let req = Request::new(1, "tools/call", Some(json!({"name": "x"})));
        let back: Request = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn response_into_result_maps_ok_and_err() {
        let ok = Response {
            jsonrpc: "2.0".into(),
            id: Some(1),
            result: Some(json!({"v": 1})),
            error: None,
        };
        assert_eq!(ok.into_result().unwrap(), json!({"v": 1}));

        let err = Response {
            jsonrpc: "2.0".into(),
            id: Some(1),
            result: None,
            error: Some(RpcError {
                code: -32601,
                message: "Method not found".into(),
                data: None,
            }),
        };
        let e = err.into_result().unwrap_err();
        assert_eq!(e.code, -32601);
        assert!(e.to_string().contains("Method not found"));
    }

    #[test]
    fn missing_result_and_error_collapses_to_null() {
        let r = Response {
            jsonrpc: "2.0".into(),
            id: Some(2),
            result: None,
            error: None,
        };
        assert_eq!(r.into_result().unwrap(), Value::Null);
    }

    #[test]
    fn idgen_is_monotonic_from_one() {
        let gen = IdGen::default();
        assert_eq!(gen.next(), 1);
        assert_eq!(gen.next(), 2);
        assert_eq!(gen.next(), 3);
    }

    #[test]
    fn encode_line_is_newline_terminated_and_parsable() {
        let line = encode_line(&Request::new(1, "ping", None)).unwrap();
        assert!(line.ends_with('\n'));
        assert!(!line[..line.len() - 1].contains('\n'), "single line");
        let back: Request = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(back.method, "ping");
    }

    #[test]
    fn notification_has_no_id_field() {
        let n = Notification::new("notifications/initialized", None);
        let v: Value = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
        assert!(v.get("id").is_none(), "notifications carry no id");
        assert_eq!(v["method"], "notifications/initialized");
    }

    /// Regression test: per JSON-RPC 2.0, a server that can't determine a
    /// malformed request's id responds with `"id": null`. When `Response.id`
    /// was a plain `i64`, that line failed to deserialize as a `Response` at
    /// all and was silently treated as an unrelated notification/log line —
    /// the client looped forever waiting for a response that could never
    /// correlate. It must parse cleanly into `id: None`.
    #[test]
    fn response_with_null_id_parses() {
        let line = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}"#;
        let resp: Response = serde_json::from_str(line).unwrap();
        assert_eq!(resp.id, None);
        let e = resp.into_result().unwrap_err();
        assert_eq!(e.code, -32700);
    }
}
