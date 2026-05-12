//! GitHub App installation ledger — Sprint O.
//!
//! Tracks which GitHub accounts have installed the lopi GitHub App.
//! Each installation maps to a customer_id which is the tenancy key for
//! `MemoryStore::open_for_customer()`.

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::MemoryStore;

/// A GitHub App installation record.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InstallationRow {
    pub id: String,
    /// GitHub installation_id from the webhook payload.
    pub installation_id: i64,
    /// Derived customer key — used to isolate per-customer SQLite stores.
    pub customer_id: String,
    /// GitHub account or org login that installed the App.
    pub account_login: String,
    /// `'User'` or `'Organization'`.
    pub account_type: String,
    /// `'active'`, `'suspended'`, or `'deleted'`.
    pub status: String,
    pub installed_at: String,
    pub updated_at: String,
}

impl MemoryStore {
    /// Upsert a GitHub App installation event.
    ///
    /// Called on `installation.created` webhook events. On `installation.deleted`,
    /// call `delete_installation()` instead.
    ///
    /// # Errors
    ///
    /// Returns an error if the database write fails.
    pub async fn upsert_installation(
        &self,
        installation_id: i64,
        account_login: &str,
        account_type: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let customer_id = sanitise_customer_id(account_login);
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO github_installations \
             (id, installation_id, customer_id, account_login, account_type, \
              status, installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6) \
             ON CONFLICT(installation_id) DO UPDATE SET \
               status = 'active', account_login = ?4, updated_at = ?6",
        )
        .bind(&id)
        .bind(installation_id)
        .bind(&customer_id)
        .bind(account_login)
        .bind(account_type)
        .bind(&now)
        .execute(&self.write_pool)
        .await?;
        Ok(customer_id)
    }

    /// Mark an installation as deleted (App uninstalled).
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn delete_installation(&self, installation_id: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE github_installations SET status = 'deleted', updated_at = ?1 \
             WHERE installation_id = ?2",
        )
        .bind(&now)
        .bind(installation_id)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Look up a customer_id from an installation_id.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn customer_for_installation(
        &self,
        installation_id: i64,
    ) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT customer_id FROM github_installations \
             WHERE installation_id = ?1 AND status = 'active' LIMIT 1",
        )
        .bind(installation_id)
        .fetch_optional(&self.read_pool)
        .await?;
        Ok(row.map(|(c,)| c))
    }

    /// List all active installations.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn list_installations(&self) -> Result<Vec<InstallationRow>> {
        sqlx::query_as::<_, InstallationRow>(
            "SELECT id, installation_id, customer_id, account_login, account_type, \
             status, installed_at, updated_at \
             FROM github_installations WHERE status = 'active' ORDER BY installed_at DESC",
        )
        .fetch_all(&self.read_pool)
        .await
        .map_err(Into::into)
    }
}

/// Derive a safe customer_id from a GitHub login.
/// GitHub logins are [a-zA-Z0-9-] but we lowercase and replace dashes for filesystem safety.
fn sanitise_customer_id(login: &str) -> String {
    login
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn upsert_creates_installation() {
        let s = store().await;
        let customer_id = s.upsert_installation(12345, "acme-corp", "Organization").await.unwrap();
        assert_eq!(customer_id, "acme-corp");
        let found = s.customer_for_installation(12345).await.unwrap();
        assert_eq!(found, Some("acme-corp".into()));
    }

    #[tokio::test]
    async fn delete_installation_hides_from_lookup() {
        let s = store().await;
        s.upsert_installation(99, "alice", "User").await.unwrap();
        s.delete_installation(99).await.unwrap();
        let found = s.customer_for_installation(99).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn upsert_idempotent_on_reinstall() {
        let s = store().await;
        s.upsert_installation(77, "bob", "User").await.unwrap();
        s.delete_installation(77).await.unwrap();
        // Reinstall — should set status back to active.
        s.upsert_installation(77, "bob", "User").await.unwrap();
        let found = s.customer_for_installation(77).await.unwrap();
        assert_eq!(found, Some("bob".into()));
    }

    #[tokio::test]
    async fn list_installations_active_only() {
        let s = store().await;
        s.upsert_installation(1, "a", "User").await.unwrap();
        s.upsert_installation(2, "b", "User").await.unwrap();
        s.delete_installation(2).await.unwrap();
        let list = s.list_installations().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].account_login, "a");
    }

    #[test]
    fn sanitise_strips_unsafe_chars() {
        assert_eq!(sanitise_customer_id("Acme Corp!"), "acme_corp_");
        assert_eq!(sanitise_customer_id("my-org"), "my-org");
        assert_eq!(sanitise_customer_id("user123"), "user123");
    }
}
