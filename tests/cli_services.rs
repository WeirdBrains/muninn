//! JSON and service ingest end-to-end (sqlite file, local HTTP mock).
mod common;
use common::*;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const T0: i64 = 1_502_942_400_000;

fn bar_json(i: i64) -> String {
    format!(r#"{{"time":{},"open":1.0,"high":2.0,"low":0.5,"close":1.5,"volume":3.0}}"#, T0 + i * 300_000)
}

#[test]
fn json_file_ingest_with_records_path() {
    let e = Env::new();
    let f = e.dir.path().join("ETHUSDT_5m.json");
    let bars: Vec<String> = (0..20).map(bar_json).collect();
    std::fs::write(&f, format!(r#"{{"data":{{"bars":[{}]}}}}"#, bars.join(","))).unwrap();
    let out = e.ok(&["-y", "ingest", "json", "--path", f.to_str().unwrap(), "--records", "data.bars", "--source", "test"]);
    assert!(out.contains("test ETH/USDT 5m"), "{out}");
    assert!(out.contains("20"), "{out}");
}

#[test]
fn sqlite_table_and_query() {
    let e = Env::new();
    let db = e.dir.path().join("exchange.sqlite");
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute_batch("CREATE TABLE BTCUSDT_1h (ts INTEGER, open REAL, high REAL, low REAL, close REAL, volume REAL);").unwrap();
    for i in 0..48i64 {
        c.execute("INSERT INTO BTCUSDT_1h VALUES (?1, 1, 2, 0.5, 1.5, 3)", [T0 + i * 3_600_000]).unwrap();
    }
    drop(c);
    let p = db.to_str().unwrap();
    let out = e.ok(&["-y", "ingest", "service", "sqlite", "--path", p, "--table", "BTCUSDT_1h"]);
    assert!(out.contains("exchange BTC/USDT 1h"), "{out}");
    assert!(out.contains("48"), "{out}");
    // a query over the same rows: all duplicates
    let out = e.ok(&["-y", "ingest", "service", "sqlite", "--path", p, "--query", "SELECT * FROM BTCUSDT_1h WHERE ts < 1503000000000", "--symbol", "BTC/USDT", "--force"]);
    assert!(out.contains("duplicate"), "{out}");
    // missing file is a clear error
    let o = e.run(&["-y", "ingest", "service", "sqlite", "--path", "/nonexistent.db", "--table", "x"]);
    assert!(!o.status.success());
}

#[test]
fn http_cursor_paging_sends_user_agent() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(2) {
            let mut s = stream.unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let mut ua = String::new();
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("user-agent:") {
                    ua = v.trim().to_string();
                }
            }
            let page2 = line.contains("cursor=p2");
            seen2.lock().unwrap().push((line.clone(), ua));
            let (bars, next) = if page2 { ((10..20).map(bar_json).collect::<Vec<_>>(), "null".to_string()) } else { ((0..10).map(bar_json).collect(), "\"p2\"".into()) };
            let body = format!(r#"{{"result":[{}],"next":{}}}"#, bars.join(","), next);
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        }
    });
    let e = Env::new();
    let url = format!("http://127.0.0.1:{port}/api/klines");
    let out = e.ok(&["-y", "ingest", "service", "http", "--url", &url, "--records", "result", "--paginate", "cursor", "--symbol", "SOL/USDT", "--duration", "5m"]);
    assert!(out.contains("127.0.0.1 SOL/USDT 5m"), "{out}");
    assert!(out.contains("20"), "{out}");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|(_, ua)| ua.starts_with("muninn/")), "{seen:?}");
    assert!(seen[1].0.contains("cursor=p2"));
}

#[test]
fn http_body_requires_post() {
    let e = Env::new();
    let o = e.run(&["-y", "ingest", "service", "http", "--url", "http://127.0.0.1:9/x", "--body", "{}"]);
    assert!(!o.status.success());
}
