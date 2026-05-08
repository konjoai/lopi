//! lopi-ui build script.
//!
//! Ensures `web/dist/` exists so `rust-embed` can compile even when the
//! SvelteKit Forge UI hasn't been built yet. Without this, a fresh clone
//! would fail at `cargo build` with a missing-folder error from the embed
//! macro.
//!
//! When the directory is empty, the runtime static handler serves the
//! placeholder page from `placeholder.html` instead. To enable the full
//! Forge UI: `cd web && npm install && npm run build`.

use std::fs;
use std::path::Path;

fn main() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let dist = Path::new(manifest).join("../../web/dist");

    if !dist.exists() {
        if let Err(e) = fs::create_dir_all(&dist) {
            println!("cargo:warning=could not create web/dist/: {e}");
        }
    }

    // Re-embed when the SvelteKit build output changes.
    println!("cargo:rerun-if-changed=../../web/dist");
}
