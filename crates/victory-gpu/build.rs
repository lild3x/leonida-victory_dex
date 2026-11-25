// Fingerprint of what decides a shader's translation: the translator crate's
// sources and the encoding of cache entries (`src/shader_cache.rs`). The shader
// cache is keyed by it (see `shader_cache::build`), so a rebuild that changes
// neither (the GPU back end, the launcher, a switch's code) keeps every
// translation already on disk, where the executable's own hash dropped them.
use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn main() {
    let mut files = Vec::new();
    collect(Path::new("../victory-shader/src"), &mut files);
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for file in &files {
        hasher.update(file.to_string_lossy().replace('\\', "/").as_bytes());
        hasher.update(&std::fs::read(file).unwrap_or_default());
    }
    hasher.update(&std::fs::read("src/shader_cache.rs").unwrap_or_default());
    println!("cargo:rustc-env=VICTORY_TRANSLATOR_HASH={}", hasher.finalize().to_hex());
    println!("cargo:rerun-if-changed=../victory-shader/src");
    println!("cargo:rerun-if-changed=src/shader_cache.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
