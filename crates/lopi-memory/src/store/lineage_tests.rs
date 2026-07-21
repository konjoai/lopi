//! Store tests — Sprint Successor-1 lineage persistence. Split out of
//! `tests.rs` to keep each test module under the 500-line file gate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use lopi_core::Task;
use std::str::FromStr;
use uuid::Uuid;

#[tokio::test]
async fn lineage_fields_persist_and_round_trip() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let parent = Task::new("root task");
    store.save_task(&parent, "success").await.unwrap();

    let mut child = Task::new("derived successor");
    child.parent_task = Some(parent.id);
    child.chain_depth = 1;
    store.save_task(&child, "queued").await.unwrap();

    let row = store.get_task(&child.id).await.unwrap().unwrap();
    assert_eq!(
        row.parent_task.as_deref(),
        Some(parent.id.0.to_string().as_str())
    );
    assert_eq!(row.chain_depth, 1);
}

#[tokio::test]
async fn tasks_without_lineage_default_to_null_parent_and_zero_depth() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let t = Task::new("no lineage");
    store.save_task(&t, "queued").await.unwrap();
    let row = store.get_task(&t.id).await.unwrap().unwrap();
    assert!(row.parent_task.is_none());
    assert_eq!(row.chain_depth, 0);
}

#[tokio::test]
async fn lineage_chain_walks_up_to_the_root() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let root = Task::new("root");
    store.save_task(&root, "success").await.unwrap();

    let mut child = Task::new("child");
    child.parent_task = Some(root.id);
    child.chain_depth = 1;
    store.save_task(&child, "success").await.unwrap();

    let mut grandchild = Task::new("grandchild");
    grandchild.parent_task = Some(child.id);
    grandchild.chain_depth = 2;
    store.save_task(&grandchild, "queued").await.unwrap();

    let chain = store.lineage_chain(&grandchild.id, 5).await.unwrap();
    let ids: Vec<String> = chain.iter().map(|r| r.id.clone()).collect();
    assert_eq!(
        ids,
        vec![
            grandchild.id.0.to_string(),
            child.id.0.to_string(),
            root.id.0.to_string(),
        ]
    );
}

#[tokio::test]
async fn lineage_chain_is_capped_at_max_depth() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let root = Task::new("root");
    store.save_task(&root, "success").await.unwrap();
    let mut child = Task::new("child");
    child.parent_task = Some(root.id);
    child.chain_depth = 1;
    store.save_task(&child, "success").await.unwrap();
    let mut grandchild = Task::new("grandchild");
    grandchild.parent_task = Some(child.id);
    grandchild.chain_depth = 2;
    store.save_task(&grandchild, "queued").await.unwrap();

    // Capped at 0 ancestor hops beyond the task itself — only the task.
    let chain = store.lineage_chain(&grandchild.id, 0).await.unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].id, grandchild.id.0.to_string());

    // Capped at 1 hop — the task plus its immediate parent, not the root.
    let chain = store.lineage_chain(&grandchild.id, 1).await.unwrap();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[1].id, child.id.0.to_string());
}

/// Sprint Successor-1's own migration, applied against a real database file
/// created **before** this sprint's `parent_task`/`chain_depth` columns
/// existed — not a fresh in-memory database. Proves `MemoryStore::open`'s
/// `ALTER TABLE` migration is genuinely idempotent-safe against pre-existing
/// data, and that the legacy row survives with the new columns defaulted.
#[tokio::test]
async fn migration_applies_cleanly_to_an_existing_pre_successor_database() {
    let dir = std::env::temp_dir().join(format!("lopi_successor_migration_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("legacy.db");

    // Hand-build the exact pre-Successor-1 `tasks` shape and insert a
    // legacy row, simulating a database file written before this sprint.
    {
        let url = format!("sqlite://{}", db_path.display());
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE tasks (\
                id TEXT PRIMARY KEY, goal TEXT NOT NULL, status TEXT NOT NULL, \
                created_at TEXT NOT NULL, completed_at TEXT, source TEXT NOT NULL\
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks (id, goal, status, created_at, source) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("legacy-task-id")
        .bind("a task from before this sprint")
        .bind("success")
        .bind(Utc::now().to_rfc3339())
        .bind("\"Cli\"")
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    // Reopening through the real `MemoryStore::open` must apply the new
    // migration cleanly against this pre-existing file, not fail or wipe it.
    let store = MemoryStore::open(&db_path).await.unwrap();
    let rows = store.load_history(10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].goal, "a task from before this sprint");
    assert!(rows[0].parent_task.is_none());
    assert_eq!(rows[0].chain_depth, 0);

    // And the store is now fully lineage-capable against this same file.
    let mut child = Task::new("a successor of the legacy task");
    child.chain_depth = 1;
    store.save_task(&child, "queued").await.unwrap();
    let row = store.get_task(&child.id).await.unwrap().unwrap();
    assert_eq!(row.chain_depth, 1);

    let _ = std::fs::remove_dir_all(&dir);
}
