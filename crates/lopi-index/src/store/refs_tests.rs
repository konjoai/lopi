#![allow(clippy::unwrap_used)]

use super::RefHit;
use crate::store::IndexStore;
use crate::types::{Language, NewRef, NewSymbol, SymbolKind};

fn fn_symbol(local_id: usize, name: &str) -> NewSymbol {
    NewSymbol {
        local_id,
        local_parent: None,
        lang: Language::Rust,
        kind: SymbolKind::Fn,
        name: name.into(),
        qualified_name: name.into(),
        signature: format!("fn {name}()"),
        doc_first_line: None,
        line_start: 1,
        line_end: 3,
        byte_start: 0,
        byte_end: 10,
        is_public: true,
    }
}

async fn seed_a_calls_b(store: &IndexStore) -> (i64, i64) {
    let (_, map_a) = store
        .replace_file_symbols("repo", "src/a.rs", Language::Rust, "h1", &[fn_symbol(0, "a")])
        .await
        .unwrap();
    let (_, map_b) = store
        .replace_file_symbols("repo", "src/b.rs", Language::Rust, "h2", &[fn_symbol(0, "b")])
        .await
        .unwrap();
    let refs = vec![NewRef {
        from_local_id: Some(0),
        to_name: "b".into(),
        line: 2,
    }];
    store
        .insert_file_refs("repo", "src/a.rs", &refs, &map_a)
        .await
        .unwrap();
    store.resolve_refs("repo").await.unwrap();
    (map_a[&0], map_b[&0])
}

#[tokio::test]
async fn resolve_refs_links_unique_name_match() {
    let store = IndexStore::open_in_memory().await.unwrap();
    seed_a_calls_b(&store).await;
    assert_eq!(store.ref_count("repo").await.unwrap(), 1);
}

#[tokio::test]
async fn callees_of_a_include_b() {
    let store = IndexStore::open_in_memory().await.unwrap();
    let (a_id, _b_id) = seed_a_calls_b(&store).await;
    let hits = store.callees("repo", a_id, 3).await.unwrap();
    assert_eq!(
        hits,
        vec![RefHit {
            qualified_name: "b".into(),
            path: "src/a.rs".into(),
            line: 2,
        }]
    );
}

#[tokio::test]
async fn callers_of_b_include_a() {
    let store = IndexStore::open_in_memory().await.unwrap();
    let (_a_id, b_id) = seed_a_calls_b(&store).await;
    let hits = store.callers("repo", b_id, 3).await.unwrap();
    assert_eq!(
        hits,
        vec![RefHit {
            qualified_name: "a".into(),
            path: "src/a.rs".into(),
            line: 2,
        }]
    );
}

#[tokio::test]
async fn ambiguous_name_stays_unresolved() {
    let store = IndexStore::open_in_memory().await.unwrap();
    store
        .replace_file_symbols("repo", "src/a.rs", Language::Rust, "h1", &[fn_symbol(0, "dup")])
        .await
        .unwrap();
    store
        .replace_file_symbols("repo", "src/b.rs", Language::Rust, "h2", &[fn_symbol(0, "dup")])
        .await
        .unwrap();
    let (_, map_c) = store
        .replace_file_symbols("repo", "src/c.rs", Language::Rust, "h3", &[fn_symbol(0, "caller")])
        .await
        .unwrap();
    let refs = vec![NewRef {
        from_local_id: Some(0),
        to_name: "dup".into(),
        line: 5,
    }];
    store
        .insert_file_refs("repo", "src/c.rs", &refs, &map_c)
        .await
        .unwrap();
    let resolved = store.resolve_refs("repo").await.unwrap();
    assert_eq!(resolved, 0, "ambiguous target must not be guessed");

    let caller_id = map_c[&0];
    let hits = store.callees("repo", caller_id, 1).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].qualified_name, "?dup", "unresolved text is kept");
}

#[tokio::test]
async fn depth_zero_returns_nothing() {
    let store = IndexStore::open_in_memory().await.unwrap();
    let (a_id, _) = seed_a_calls_b(&store).await;
    assert!(store.callees("repo", a_id, 0).await.unwrap().is_empty());
}

#[tokio::test]
async fn multi_hop_traversal_reaches_depth_two() {
    let store = IndexStore::open_in_memory().await.unwrap();
    let (_, map_a) = store
        .replace_file_symbols("repo", "src/a.rs", Language::Rust, "h1", &[fn_symbol(0, "a")])
        .await
        .unwrap();
    let (_, map_b) = store
        .replace_file_symbols("repo", "src/b.rs", Language::Rust, "h2", &[fn_symbol(0, "b")])
        .await
        .unwrap();
    store
        .replace_file_symbols("repo", "src/c.rs", Language::Rust, "h3", &[fn_symbol(0, "c")])
        .await
        .unwrap();

    // a -> b -> c
    store
        .insert_file_refs(
            "repo",
            "src/a.rs",
            &[NewRef {
                from_local_id: Some(0),
                to_name: "b".into(),
                line: 2,
            }],
            &map_a,
        )
        .await
        .unwrap();
    store
        .insert_file_refs(
            "repo",
            "src/b.rs",
            &[NewRef {
                from_local_id: Some(0),
                to_name: "c".into(),
                line: 2,
            }],
            &map_b,
        )
        .await
        .unwrap();
    store.resolve_refs("repo").await.unwrap();

    let a_id = map_a[&0];
    let one_hop = store.callees("repo", a_id, 1).await.unwrap();
    assert_eq!(one_hop.len(), 1, "depth 1 sees only b");

    let two_hop = store.callees("repo", a_id, 2).await.unwrap();
    assert_eq!(two_hop.len(), 2, "depth 2 sees b and c");
}

#[tokio::test]
async fn resolve_refs_for_scoped_by_touched_path() {
    let store = IndexStore::open_in_memory().await.unwrap();
    let (_, map_a) = store
        .replace_file_symbols("repo", "src/a.rs", Language::Rust, "h1", &[fn_symbol(0, "a")])
        .await
        .unwrap();
    store
        .replace_file_symbols("repo", "src/b.rs", Language::Rust, "h2", &[fn_symbol(0, "b")])
        .await
        .unwrap();
    store
        .insert_file_refs(
            "repo",
            "src/a.rs",
            &[NewRef { from_local_id: Some(0), to_name: "b".into(), line: 2 }],
            &map_a,
        )
        .await
        .unwrap();

    let resolved = store
        .resolve_refs_for("repo", &["src/a.rs".to_string()], &[])
        .await
        .unwrap();
    assert_eq!(resolved, 1, "the touched file's own fresh ref resolves");
}

#[tokio::test]
async fn resolve_refs_for_scoped_by_new_name_catches_target_added_later() {
    let store = IndexStore::open_in_memory().await.unwrap();
    let (_, map_a) = store
        .replace_file_symbols("repo", "src/a.rs", Language::Rust, "h1", &[fn_symbol(0, "a")])
        .await
        .unwrap();
    // "b" doesn't exist yet — the ref is inserted unresolved.
    store
        .insert_file_refs(
            "repo",
            "src/a.rs",
            &[NewRef { from_local_id: Some(0), to_name: "b".into(), line: 2 }],
            &map_a,
        )
        .await
        .unwrap();
    // Scoping by a.rs (already indexed, not "touched" this pass) with
    // no new names finds nothing — nothing changed that could resolve it.
    let none_yet = store.resolve_refs_for("repo", &[], &[]).await.unwrap();
    assert_eq!(none_yet, 0);

    // Now "b" appears, in a different file the caller correctly lists
    // as this pass's only new symbol name (not a touched path).
    store
        .replace_file_symbols("repo", "src/b.rs", Language::Rust, "h2", &[fn_symbol(0, "b")])
        .await
        .unwrap();
    let resolved = store
        .resolve_refs_for("repo", &["src/b.rs".to_string()], &["b".to_string()])
        .await
        .unwrap();
    assert_eq!(resolved, 1, "the pre-existing dangling ref to `b` resolves once `b` exists");
}

#[tokio::test]
async fn resolve_refs_for_ignores_unrelated_unresolved_backlog() {
    let store = IndexStore::open_in_memory().await.unwrap();
    let (_, map_a) = store
        .replace_file_symbols("repo", "src/a.rs", Language::Rust, "h1", &[fn_symbol(0, "a")])
        .await
        .unwrap();
    // A permanently-unresolvable ref (calls something never indexed) —
    // simulates the external-crate/stdlib-call backlog.
    store
        .insert_file_refs(
            "repo",
            "src/a.rs",
            &[NewRef { from_local_id: Some(0), to_name: "external_fn".into(), line: 2 }],
            &map_a,
        )
        .await
        .unwrap();

    // An unrelated later pass touches a different file and adds an
    // unrelated symbol — must not re-scan (or spuriously resolve) the
    // unrelated backlog entry.
    store
        .replace_file_symbols("repo", "src/z.rs", Language::Rust, "h9", &[fn_symbol(0, "z")])
        .await
        .unwrap();
    let resolved = store
        .resolve_refs_for("repo", &["src/z.rs".to_string()], &["z".to_string()])
        .await
        .unwrap();
    assert_eq!(resolved, 0, "unrelated pass touches nothing that could resolve external_fn");
}
