#![allow(clippy::unwrap_used)]

use super::*;
use crate::types::{NewSymbol, SymbolKind};


    fn sym(local_id: usize, name: &str, doc: &str) -> NewSymbol {
        NewSymbol {
            local_id,
            local_parent: None,
            lang: Language::Rust,
            kind: SymbolKind::Fn,
            name: name.into(),
            qualified_name: name.into(),
            signature: format!("pub fn {name}()"),
            doc_first_line: Some(doc.into()),
            line_start: 1,
            line_end: 5,
            byte_start: 0,
            byte_end: 10,
            is_public: true,
        }
    }

    #[tokio::test]
    async fn find_ranks_and_bounds_results() {
        let store = IndexStore::open_in_memory().await.unwrap();
        store
            .replace_file_symbols(
                "repo",
                "src/lib.rs",
                Language::Rust,
                "h",
                &[sym(0, "run_agent", "Runs the agent."), sym(1, "stop", "Stops it.")],
            )
            .await
            .unwrap();

        let result = find(&store, "repo", "run", None, None, None, 10).await.unwrap();
        assert!(!result.items.is_empty());
        assert_eq!(result.items[0].qualified_name, "run_agent");
        assert!(!result.truncated);
    }

    #[tokio::test]
    async fn find_reports_truncation_honestly() {
        let store = IndexStore::open_in_memory().await.unwrap();
        let syms: Vec<_> = (0..5).map(|i| sym(i, &format!("fn_{i}"), "doc")).collect();
        store.replace_file_symbols("repo", "src/lib.rs", Language::Rust, "h", &syms).await.unwrap();

        let result = find(&store, "repo", "", None, None, None, 2).await.unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.total_matches, 5);
        assert!(result.truncated);
    }

    #[tokio::test]
    async fn read_returns_exact_span_by_qualified_name() {
        let store = IndexStore::open_in_memory().await.unwrap();
        let mut s = sym(0, "run", "doc");
        s.line_start = 2;
        s.line_end = 4;
        store.replace_file_symbols("repo", "lib.rs", Language::Rust, "h", &[s]).await.unwrap();

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("lib.rs"), "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let result = read(
            &store,
            "repo",
            dir.path(),
            ReadTarget::QualifiedName("run".into()),
            0,
            400,
        )
        .await
        .unwrap();
        assert_eq!(result.text, "line2\nline3\nline4");
        assert!(!result.truncated);
    }

    #[tokio::test]
    async fn read_by_span_needs_no_index_lookup() {
        let store = IndexStore::open_in_memory().await.unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("f.rs"), "a\nb\nc\n").unwrap();

        let result = read(
            &store,
            "repo",
            dir.path(),
            ReadTarget::Span { path: "f.rs".into(), line_start: 1, line_end: 2 },
            0,
            400,
        )
        .await
        .unwrap();
        assert_eq!(result.text, "a\nb");
    }

    #[tokio::test]
    async fn read_elides_and_reports_continuation_over_budget() {
        let store = IndexStore::open_in_memory().await.unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let body: String = (1..=100).map(|i| format!("line{i}\n")).collect();
        std::fs::write(dir.path().join("big.rs"), &body).unwrap();

        let result = read(
            &store,
            "repo",
            dir.path(),
            ReadTarget::Span { path: "big.rs".into(), line_start: 1, line_end: 100 },
            0,
            10,
        )
        .await
        .unwrap();
        assert!(result.truncated);
        assert!(result.text.contains("elided"));
        assert!(result.continue_from.is_some());
    }

    #[tokio::test]
    async fn read_unknown_qualified_name_errors() {
        let store = IndexStore::open_in_memory().await.unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let err = read(
            &store,
            "repo",
            dir.path(),
            ReadTarget::QualifiedName("nope".into()),
            0,
            400,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("nope"));
    }

    #[tokio::test]
    async fn composite_query_expands_refs_per_hit() {
        let store = IndexStore::open_in_memory().await.unwrap();
        let (_, map_a) = store
            .replace_file_symbols("repo", "a.rs", Language::Rust, "h1", &[sym(0, "a", "doc")])
            .await
            .unwrap();
        store
            .replace_file_symbols("repo", "b.rs", Language::Rust, "h2", &[sym(0, "b", "doc")])
            .await
            .unwrap();
        store
            .insert_file_refs(
                "repo",
                "a.rs",
                &[crate::types::NewRef { from_local_id: Some(0), to_name: "b".into(), line: 2 }],
                &map_a,
            )
            .await
            .unwrap();
        store.resolve_refs("repo").await.unwrap();

        let spec = QuerySpec {
            find_text: "a".into(),
            kind: None,
            lang: None,
            path_glob: None,
            limit: 10,
            then_refs: Some(RefDirection::Callees),
            refs_depth: 1,
        };
        let result = composite_query(&store, "repo", &spec).await.unwrap();
        assert_eq!(result.items.len(), 1);
        let refs_env = result.items[0].refs.as_ref().unwrap();
        assert_eq!(refs_env.items.len(), 1);
        assert_eq!(refs_env.items[0].qualified_name, "b");
    }
