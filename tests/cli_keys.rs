//! keys CLI: init -> words -> recover, shards, rotation, wrong password.
mod common;
use common::*;

#[test]
fn init_rotate_recover_via_cli() {
    let e = Env::bare();
    let out = e.ok(&["keys", "init", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let words = v["words"].as_str().unwrap().to_string();
    let shards: Vec<String> = v["shards"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
    assert_eq!(words.split_whitespace().count(), 24);
    assert_eq!(shards.len(), 5);
    assert!(e.run(&["keys", "init"]).status.code() != Some(0));

    let f = fixture("ETHUSDT_5m.csv");
    e.ok(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);

    // wrong password fails cleanly
    let o = e.cmd().env("MUNINN_PASSWORD", "nope").args(["sources", "list"]).output().unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("wrong password"));

    // rotate with words from a file
    let wf = e.dir.path().join("words.txt");
    std::fs::write(&wf, &words).unwrap();
    e.ok(&["-y", "keys", "rotate", "--words-file", wf.to_str().unwrap()]);
    assert!(e.ok(&["keys", "status"]).contains("service key      v2"));
    assert!(e.ok(&["sources", "list"]).contains("binance"));

    // recover with 3 shards and a new password
    let mut args = vec!["keys".to_string(), "recover".to_string()];
    for (i, s) in shards.iter().enumerate().skip(2) {
        let p = e.dir.path().join(format!("s{i}"));
        std::fs::write(&p, s).unwrap();
        args.push("--shard".into());
        args.push(p.display().to_string());
    }
    let o = e.cmd().env("MUNINN_PASSWORD", "new pw").args(&args).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let o = e.cmd().env("MUNINN_PASSWORD", "new pw").args(["sources", "list"]).output().unwrap();
    assert!(o.status.success());

    // 2 shards fail
    let o = e.run(&["keys", "recover", "--shard", e.dir.path().join("s2").to_str().unwrap(), "--shard", e.dir.path().join("s3").to_str().unwrap()]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("at least 3"));

    // export everything to one dir needs -y
    let ex = e.dir.path().join("exp");
    let o = e.run(&["keys", "export", "--out", ex.to_str().unwrap(), "--words-file", wf.to_str().unwrap()]);
    assert!(!o.status.success());
    e.ok(&["-y", "keys", "export", "--out", ex.to_str().unwrap(), "--words-file", wf.to_str().unwrap(), "--only", "master,words"]);
    assert!(ex.join("master.key.age").exists());
}

#[test]
fn commands_require_init() {
    let e = Env::bare();
    let o = e.run(&["sources", "list"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("keys init"));
}
