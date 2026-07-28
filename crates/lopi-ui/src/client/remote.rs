//! `RemoteClient` — a [`TuiClient`] over HTTP against a running `lopi sail`
//! server. Refactors the previous bespoke `reqwest_cancel` free function
//! (`src/remote.rs`) into a proper client method, and is the only client
//! implementation web/macOS/iOS's own submission paths are functionally
//! equivalent to — authoritative when `RemoteClient`/`LocalClient` behavior
//! ever diverges (see `LEDGER.md`).

use super::{ChainSummary, ClientError, TaskSummary, TuiClient};
use crate::web::types::{CreateTaskRequest, CreateTaskResponse};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde_json::json;

/// An HTTP [`TuiClient`] against a `lopi sail` server's REST API.
pub struct RemoteClient {
    http: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl RemoteClient {
    /// Build a client against `base_url` (e.g. `"http://127.0.0.1:3000"`),
    /// sending `Authorization: Bearer <token>` on every request when
    /// `token` is set and non-empty.
    #[must_use]
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.filter(|t| !t.is_empty()),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn authorize(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => builder.bearer_auth(t),
            None => builder,
        }
    }

    /// Translate a response into `Ok(T)` or a typed [`ClientError`],
    /// distinguishing "no token configured" (checked before the request is
    /// even sent, by callers), "401 from server," and every other server
    /// failure. `op` names the operation for error messages only.
    async fn parse<T: DeserializeOwned>(
        &self,
        resp: Result<reqwest::Response, reqwest::Error>,
        op: &str,
    ) -> Result<T, ClientError> {
        let resp = resp.map_err(|e| ClientError::Transport(e.to_string()))?;
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized(op.to_string()));
        }
        if status == StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound(op.to_string()));
        }
        if status == StatusCode::CONFLICT {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Conflict(body));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Other(format!("{op}: {status} {body}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| ClientError::Transport(format!("{op}: {e}")))
    }

    /// Same as [`Self::parse`] but for endpoints with no meaningful
    /// response body — success is `status.is_success()`, nothing else.
    async fn parse_unit(
        &self,
        resp: Result<reqwest::Response, reqwest::Error>,
        op: &str,
    ) -> Result<(), ClientError> {
        let resp = resp.map_err(|e| ClientError::Transport(e.to_string()))?;
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized(op.to_string()));
        }
        if status == StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound(op.to_string()));
        }
        if status == StatusCode::CONFLICT {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Conflict(body));
        }
        if status.is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(ClientError::Other(format!("{op}: {status} {body}")))
        }
    }
}

#[async_trait]
impl TuiClient for RemoteClient {
    async fn list_tasks(&self) -> Result<Vec<TaskSummary>, ClientError> {
        let resp = self
            .authorize(self.http.get(self.url("/api/tasks")))
            .send()
            .await;
        let body: serde_json::Value = self.parse(resp, "list_tasks").await?;
        let tasks = body.get("tasks").cloned().unwrap_or(json!([]));
        serde_json::from_value(tasks).map_err(|e| ClientError::Transport(e.to_string()))
    }

    async fn get_task(&self, id: &str) -> Result<TaskSummary, ClientError> {
        let resp = self
            .authorize(self.http.get(self.url(&format!("/api/tasks/{id}"))))
            .send()
            .await;
        self.parse(resp, "get_task").await
    }

    async fn create_task(&self, request: &CreateTaskRequest) -> Result<String, ClientError> {
        let resp = self
            .authorize(self.http.post(self.url("/api/tasks")).json(request))
            .send()
            .await;
        let body: CreateTaskResponse = self.parse(resp, "create_task").await?;
        Ok(body.duplicate_of.unwrap_or(body.id))
    }

    async fn cancel_task(&self, id: &str) -> Result<bool, ClientError> {
        let resp = self
            .authorize(self.http.delete(self.url(&format!("/api/tasks/{id}"))))
            .send()
            .await;
        let body: serde_json::Value = self.parse(resp, "cancel_task").await?;
        Ok(body
            .get("cancelled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false))
    }

    async fn approve_plan(&self, id: &str) -> Result<(), ClientError> {
        let resp = self
            .authorize(
                self.http
                    .post(self.url(&format!("/api/tasks/{id}/plan/approve"))),
            )
            .send()
            .await;
        self.parse_unit(resp, "approve_plan").await
    }

    async fn reject_plan(&self, id: &str) -> Result<(), ClientError> {
        let resp = self
            .authorize(
                self.http
                    .post(self.url(&format!("/api/tasks/{id}/plan/reject"))),
            )
            .send()
            .await;
        self.parse_unit(resp, "reject_plan").await
    }

    async fn list_chains(&self) -> Result<Vec<ChainSummary>, ClientError> {
        let resp = self
            .authorize(self.http.get(self.url("/api/schedule-chains")))
            .send()
            .await;
        let body: serde_json::Value = self.parse(resp, "list_chains").await?;
        let chains = body.get("chains").cloned().unwrap_or(json!([]));
        serde_json::from_value(chains).map_err(|e| ClientError::Transport(e.to_string()))
    }

    async fn get_chain(&self, id: &str) -> Result<ChainSummary, ClientError> {
        let resp = self
            .authorize(
                self.http
                    .get(self.url(&format!("/api/schedule-chains/{id}"))),
            )
            .send()
            .await;
        self.parse(resp, "get_chain").await
    }

    async fn create_chain(&self, body: serde_json::Value) -> Result<ChainSummary, ClientError> {
        let resp = self
            .authorize(self.http.post(self.url("/api/schedule-chains")).json(&body))
            .send()
            .await;
        self.parse(resp, "create_chain").await
    }

    async fn enable_chain(&self, id: &str) -> Result<(), ClientError> {
        let resp = self
            .authorize(
                self.http
                    .post(self.url(&format!("/api/schedule-chains/{id}/enable"))),
            )
            .send()
            .await;
        self.parse_unit(resp, "enable_chain").await
    }

    async fn disable_chain(&self, id: &str) -> Result<(), ClientError> {
        let resp = self
            .authorize(
                self.http
                    .post(self.url(&format!("/api/schedule-chains/{id}/disable"))),
            )
            .send()
            .await;
        self.parse_unit(resp, "disable_chain").await
    }

    async fn run_chain_now(&self, id: &str) -> Result<(), ClientError> {
        let resp = self
            .authorize(
                self.http
                    .post(self.url(&format!("/api/schedule-chains/{id}/run-now"))),
            )
            .send()
            .await;
        self.parse_unit(resp, "run_chain_now").await
    }

    async fn get_loop_config(&self) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .authorize(self.http.get(self.url("/api/loop-engineering")))
            .send()
            .await;
        self.parse(resp, "get_loop_config").await
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
