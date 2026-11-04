//! Embed every docs/**/*.md file (except docs/historical/) as a help topic (single source of truth for docs and in-app help).
use std::fs;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() && p.file_name().is_some_and(|n| n == "historical") {
            continue; // archived snapshots are not help topics
        }
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

fn main() {
    grpc();
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let docs = root.join("docs");
    println!("cargo:rerun-if-changed={}", docs.display());
    let mut files = vec![];
    walk(&docs, &mut files);
    let mut src = String::from("/// (topic id, markdown) generated from docs/ by build.rs\npub const TOPICS: &[(&str, &str)] = &[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let rel = f.strip_prefix(&docs).unwrap().with_extension("");
        let id = rel.to_string_lossy().replace('\\', "/");
        src.push_str(&format!("    ({:?}, include_str!({:?})),\n", id, f.display().to_string()));
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("help_topics.rs");
    fs::write(out, src).unwrap();
}

/// The `muninn.live.v1` stream service, from the .proto with a pure-Rust compiler (no protoc).
/// The messages are hand-written in `src/collab/proto.rs`; only the service is generated.
fn grpc() {
    let proto = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../proto");
    println!("cargo:rerun-if-changed={}", proto.display());
    let fds = protox::compile(["muninn/live/v1/live.proto"], [proto]).expect("proto/muninn/live/v1/live.proto");
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .extern_path(".muninn.live.v1", "crate::collab::proto")
        .compile_fds(fds)
        .expect("generating the muninn.live.v1 service");
}
