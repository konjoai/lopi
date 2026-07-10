//! Embedded static-asset serving for the dashboard SPA.
//!
//! Split out of `web/mod.rs` to keep that module within the 500-line budget.
//! The SvelteKit "Forge" build is embedded at compile time; when no build is
//! present the bundled `placeholder.html` is served instead.

use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

/// SvelteKit Forge static build embedded into the lopi binary.
///
/// `web/dist/` is created (empty if needed) by the lopi-ui build script so
/// `cargo build` succeeds even before `npm run build`. When the directory
/// is empty at compile time, the runtime handler serves the placeholder
/// page from `placeholder.html` instead.
#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../web/dist"]
struct WebAssets;

const PLACEHOLDER_HTML: &str = include_str!("../placeholder.html");

/// Static asset handler — serves the embedded SvelteKit Forge build.
///
/// Lookup order:
///   1. Direct file match (e.g. `/_app/immutable/chunks/x.js`, `/favicon.svg`)
///   2. Append `.html` for prerendered routes (e.g. `/overview` →
///      `overview.html`)
///   3. Fall back to `index.html` (SPA client-side routing for unknown paths)
///   4. Fall back to the bundled placeholder if `web/dist/` is empty
///      (i.e. `npm run build` hasn't been run yet).
pub(super) async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/').to_string();

    // 1. Direct file
    if !path.is_empty() {
        if let Some(file) = WebAssets::get(&path) {
            return file_response(file, &path);
        }
    }

    // 2. .html fallback for prerendered routes
    if !path.is_empty() && !path.contains('.') {
        let html_path = format!("{}.html", path);
        if let Some(file) = WebAssets::get(&html_path) {
            return file_response(file, &html_path);
        }
    }

    // 3. SPA fallback — index.html handles client-side routing
    if let Some(file) = WebAssets::get("index.html") {
        return file_response(file, "index.html");
    }

    // 4. No SvelteKit build present — show placeholder with build instructions.
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(PLACEHOLDER_HTML))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Serve an embedded file with appropriate Content-Type and Cache-Control.
///
/// Cache strategy:
///   - SvelteKit's `_app/immutable/*` chunks are content-hashed → cache forever
///   - Everything else (HTML, top-level assets) → 5-minute browser cache
fn file_response(file: rust_embed::EmbeddedFile, path: &str) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache_control = if path.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=300"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from(file.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
