//! Thin GitHub REST API client for lopi — write operations only.
//!
//! Reads happen through webhook payloads (no polling). This client
//! handles the outbound side: posting triage comments, adding labels,
//! so lopi can participate in GitHub workflows without a full SDK.

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;

const GITHUB_API: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("lopi/", env!("CARGO_PKG_VERSION"));

/// GitHub API write client. Constructed once and shared via Arc.
pub struct GitHubClient {
    http: Client,
    token: String,
    /// API base URL — `GITHUB_API` in production. Overridable only via
    /// `new_with_base_url` (test-only) so a test can point this client at a
    /// local mock server instead of the real GitHub API.
    base_url: String,
}

impl GitHubClient {
    /// Construct with a personal access token or GitHub App installation token.
    ///
    /// # Errors
    ///
    /// Returns an error if the reqwest client cannot be constructed.
    pub fn new(token: impl Into<String>) -> Result<Self> {
        Self::new_with_base_url(token, GITHUB_API)
    }

    /// Construct against a caller-supplied base URL instead of the real
    /// GitHub API — the seam that lets tests point this client at a local
    /// mock server. `pub(crate)` since no production caller ever needs a
    /// non-GitHub base URL; kept out of the public API on purpose.
    ///
    /// # Errors
    ///
    /// Returns an error if the reqwest client cannot be constructed.
    fn new_with_base_url(token: impl Into<String>, base_url: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .context("building GitHub HTTP client")?;
        Ok(Self {
            http,
            token: token.into(),
            base_url: base_url.into(),
        })
    }

    /// Post a comment on a GitHub issue or pull request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or GitHub returns a non-2xx status.
    pub async fn post_comment(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> Result<()> {
        let base = &self.base_url;
        let url = format!("{base}/repos/{owner}/{repo}/issues/{issue_number}/comments");
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&json!({ "body": body }))
            .send()
            .await
            .context("POST GitHub comment")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub comment failed {status}: {text}");
        }
        tracing::info!(owner, repo, issue_number, "posted GitHub comment");
        Ok(())
    }

    /// Add one or more labels to an issue or PR.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or GitHub returns a non-2xx status.
    pub async fn add_labels(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        labels: &[&str],
    ) -> Result<()> {
        let base = &self.base_url;
        let url = format!("{base}/repos/{owner}/{repo}/issues/{issue_number}/labels");
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&json!({ "labels": labels }))
            .send()
            .await
            .context("POST GitHub labels")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub add_labels failed {status}: {text}");
        }
        tracing::info!(owner, repo, issue_number, ?labels, "added GitHub labels");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn client_constructs_with_token() {
        let c = GitHubClient::new("ghp_test_token");
        assert!(c.is_ok());
    }

    #[test]
    fn client_constructs_with_empty_token() {
        // Empty token is allowed at construction time — GitHub will reject at API call time.
        let c = GitHubClient::new("");
        assert!(c.is_ok());
    }

    async fn mock_client(server: &MockServer) -> GitHubClient {
        GitHubClient::new_with_base_url("test_token", server.uri()).unwrap()
    }

    #[tokio::test]
    async fn post_comment_sends_the_expected_request_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/acme/widgets/issues/42/comments"))
            .and(header("authorization", "Bearer test_token"))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client
            .post_comment("acme", "widgets", 42, "hello from lopi")
            .await
            .unwrap();
        // `.expect(1)` above is verified when `server` drops at end of scope.
    }

    #[tokio::test]
    async fn post_comment_sends_the_body_as_json() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/acme/widgets/issues/42/comments"))
            .and(wiremock::matchers::body_json(
                serde_json::json!({ "body": "hello from lopi" }),
            ))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client
            .post_comment("acme", "widgets", 42, "hello from lopi")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn post_comment_surfaces_a_non_2xx_status_as_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/acme/widgets/issues/42/comments"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client
            .post_comment("acme", "widgets", 42, "hello")
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("404"), "{msg}");
        assert!(msg.contains("Not Found"), "{msg}");
    }

    #[tokio::test]
    async fn add_labels_sends_the_expected_request_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/acme/widgets/issues/7/labels"))
            .and(header("authorization", "Bearer test_token"))
            .and(wiremock::matchers::body_json(
                serde_json::json!({ "labels": ["bug", "triaged"] }),
            ))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client
            .add_labels("acme", "widgets", 7, &["bug", "triaged"])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn add_labels_surfaces_a_non_2xx_status_as_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/acme/widgets/issues/7/labels"))
            .respond_with(ResponseTemplate::new(422).set_body_string("Validation Failed"))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client
            .add_labels("acme", "widgets", 7, &["bug"])
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("422"), "{msg}");
        assert!(msg.contains("Validation Failed"), "{msg}");
    }
}
