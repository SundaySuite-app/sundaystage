fn main() {
    // Phase 5.2 — the crash-isolated output process ships in the bundle as the
    // `output-process` externalBin (built from `src/bin/output.rs` and staged
    // by `scripts/prepare-output-bin.mjs` in beforeBuildCommand). tauri-build
    // requires the triple-suffixed file to exist for *every* compile, so plain
    // `cargo build/test/clippy` gets an empty placeholder. The runtime spawn
    // path ignores empty files (see `output::process::output_binary_path`), so
    // a placeholder can never be executed by accident.
    //
    // Note the sidecar file name intentionally differs from the cargo bin name
    // (`sundaystage-output`): tauri-build copies externalBin into the cargo
    // target dir, and an identical name would clobber the freshly built dev
    // binary with the placeholder.
    let triple = std::env::var("TARGET").expect("cargo sets TARGET");
    let ext = if triple.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let dir = std::path::Path::new("binaries");
    std::fs::create_dir_all(dir).expect("create binaries dir");
    let placeholder = dir.join(format!("output-process-{triple}{ext}"));
    if !placeholder.exists() {
        std::fs::write(&placeholder, b"").expect("write externalBin placeholder");
    }

    tauri_build::build()
}
