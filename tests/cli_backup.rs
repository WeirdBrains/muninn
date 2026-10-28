//! Backup/restore round trip including preferences and themes.
mod common;
use common::*;

#[test]
fn backup_restore_round_trip() {
    let e = Env::new();
    let f = fixture("ETHUSDT_5m.csv");
    e.ok(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    let home = e.home();
    std::fs::create_dir_all(home.join("themes/mine/assets")).unwrap();
    std::fs::write(home.join("themes/mine/interface.toml"), "# custom").unwrap();
    std::fs::write(home.join("preferences.toml"), "[theme]\nname = \"mine\"\nmode = \"dark\"\n").unwrap();
    let bk = e.dir.path().join("bk");
    e.ok(&["backup", "create", "--out", bk.to_str().unwrap()]);
    e.ok(&["backup", "create", "--out", bk.to_str().unwrap(), "--encrypt"]);

    // Restore into a brand-new home.
    let e2 = Env::bare();
    e2.ok(&["backup", "restore", "--from", bk.to_str().unwrap()]);
    let h2 = e2.home();
    assert!(std::fs::read_to_string(h2.join("preferences.toml")).unwrap().contains("mine"));
    assert_eq!(std::fs::read_to_string(h2.join("themes/mine/interface.toml")).unwrap(), "# custom");
    assert!(h2.join("keys/envelope.toml").exists());
    let csv = e2.ok(&["export", "--format", "csv", "--out", "-"]);
    assert_eq!(csv.lines().count(), 301);

    // Restoring over existing data needs confirmation.
    let o = e2.run(&["backup", "restore", "--from", bk.to_str().unwrap()]);
    assert!(!o.status.success());
    e2.ok(&["-y", "backup", "restore", "--from", bk.to_str().unwrap()]);

    // Corruption is detected.
    let file = std::fs::read_dir(&bk).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().unwrap() == "zst").unwrap();
    let mut b = std::fs::read(&file).unwrap();
    let n = b.len() / 2;
    b[n] ^= 0xff;
    let bad = e.dir.path().join("muninn-backup-bad.tar.zst");
    std::fs::write(&bad, b).unwrap();
    assert!(!Env::bare().run(&["backup", "restore", "--from", bad.to_str().unwrap()]).status.success());
}
