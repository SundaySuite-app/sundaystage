//! The process-wide rustls crypto provider.
//!
//! reqwest 0.13 runs on `rustls-no-provider` (see Cargo.toml). In that mode it
//! PANICS on client construction when no rustls crypto provider is installed in
//! the process — it does not pick one from the compiled-in features, and the
//! panic happens inside `build()`, so a `.build().ok()?` guard does not catch
//! it. tauri-plugin-updater installs `ring` the same way right before it builds
//! its own client, but nothing guarantees the updater has run before the first
//! telemetry flush or library publish — so every client construction calls this
//! first.

/// Install `ring` as the process-wide rustls crypto provider, once.
/// Idempotent: a provider that is already installed (ours or the updater's) wins.
pub fn ensure_rustls_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_client_builds_in_a_process_with_no_provider_installed() {
        // A test process starts with no provider installed — exactly like a fresh
        // launch before the updater has run. Without `ensure_rustls_provider` the
        // `build()` below panics inside reqwest; it compiles fine either way.
        super::ensure_rustls_provider();
        reqwest::Client::builder()
            .build()
            .expect("the TLS backend must initialise once the provider is installed");
    }
}
