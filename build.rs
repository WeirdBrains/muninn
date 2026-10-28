//! Embed every docs/**/*.md file as a help topic (single source of truth for docs and in-app help).
use std::fs;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let docs = root.join("docs");
    println!("cargo:rerun-if-changed=docs");
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
