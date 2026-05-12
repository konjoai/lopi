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
}

impl GitHubClient {
    /// Construct with a personal access token or GitHub App installation token.
    ///
    /// # Errors
    ///
    /// Returns an error if the reqwest client cannot be constructed.
    pub fn new(token: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .context("building GitHub HTTP client")?;
        Ok(Self {
            http,
            token: token.into(),
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
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/issues/{issue_number}/comments");
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
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/issues/{issue_number}/labels");
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
}
