// SPDX-License-Identifier: MIT

//! Build script — exposes package metadata as compile-time env vars.

fn main() {
    // Read app ID from [package.metadata] and expose it as APP_ID
    // so the crate can use env!("APP_ID") without any hardcoding.
    let app_id = std::env::var("CARGO_PKG_METADATA_APP_ID")
        .unwrap_or_else(|_| "com.github.jvcByte.clip_pop".to_owned());

    println!("cargo:rustc-env=APP_ID={app_id}");

    // Re-run only if Cargo.toml changes.
    println!("cargo:rerun-if-changed=Cargo.toml");
}
