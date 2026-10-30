//! `mnnn keys ...`
use super::{Ctx, KeysCommand, MasterInput};
use crate::Error;
use crate::crypto::keyring::{self, Envelope, ExportItem};
use crate::crypto::{self, Key32};
use secrecy::{ExposeSecret, SecretString};

/// Resolve the master key from words, shards or an exported master file.
pub fn master(ctx: &Ctx, m: &MasterInput) -> anyhow::Result<Key32> {
    let pw = || ctx.export_password("Export password").map_err(|e| Error::Other(e.to_string()));
    let key = if let Some(w) = &m.words {
        crypto::from_words(w)?
    } else if let Some(f) = &m.words_file {
        crypto::from_words(&keyring::read_maybe_age(f, pw)?)?
    } else if let Some(f) = &m.master_file {
        Key32::from_hex(&keyring::read_maybe_age(f, pw)?)?
    } else if !m.shards.is_empty() {
        let mut pass: Option<SecretString> = None;
        let mut v = Vec::new();
        for f in &m.shards {
            let s = keyring::read_maybe_age(f, || {
                if pass.is_none() {
                    pass = Some(pw()?);
                }
                Ok(pass.clone().unwrap_or_else(|| SecretString::from("")))
            })?;
            v.push(s.to_string());
        }
        crypto::combine_shards(&v)?
    } else if ctx.io.interactive {
        let w = ctx.io.password("24 recovery words (input hidden)", "")?;
        crypto::from_words(&w)?
    } else {
        return Err(Error::missing("master key", "pass --words-file, --shard (x3) or --master-file").into());
    };
    keyring::verify_master(&ctx.paths, &key)?;
    Ok(key)
}

pub(super) fn new_password(ctx: &Ctx, question: &str) -> anyhow::Result<SecretString> {
    if let Ok(p) = std::env::var("MUNINN_PASSWORD") {
        return Ok(p.into());
    }
    let a = ctx.io.password(question, "set MUNINN_PASSWORD or run interactively")?;
    let b = ctx.io.password("Repeat password", "")?;
    if a != b {
        return Err(Error::invalid("password", "the two entries differ").into());
    }
    Ok(a.into())
}

pub fn run(ctx: &Ctx, cmd: KeysCommand) -> anyhow::Result<()> {
    match cmd {
        KeysCommand::Init { json } => {
            let pw = new_password(ctx, "New Muninn password")?;
            let out = keyring::init(&ctx.paths, &pw)?;
            if json {
                let shards: Vec<&str> = out.shards.iter().map(|s| s.as_str()).collect();
                println!(
                    "{}",
                    serde_json::json!({ "db_id": out.db_id, "words": out.words.as_str(), "shards": shards, "migrated_plain_db": out.migrated_plain_db })
                );
            } else {
                // Secrets are requested data: printed even with --quiet.
                println!("Muninn keys initialized (database id {}).", out.db_id);
                if out.migrated_plain_db {
                    println!("The existing plaintext database was encrypted in place.");
                }
                println!("\nRECOVERY WORDS (write these down; they recover the master key):\n");
                for (i, w) in out.words.split_whitespace().enumerate() {
                    print!("{:>2}. {:<10}{}", i + 1, w, if (i + 1) % 6 == 0 { "\n" } else { " " });
                }
                println!("\nDARK SHARDS (any 3 of 5 recover the master key; store them apart):\n");
                for s in &out.shards {
                    println!("  {}", s.as_str());
                }
                println!("\nThese are shown once and never stored. Use `mnnn keys export` for encrypted copies.");
            }
        }
        KeysCommand::Status => {
            let env = Envelope::require(&ctx.paths)?;
            println!("database id      {}", env.db_id);
            println!("database         {}", ctx.paths.db.display());
            println!("service key      v{}", env.service_version);
            for f in keyring::service_files(&ctx.paths)? {
                println!("  v{} {}", f.version, if f.retired { "retired" } else { "active" });
            }
        }
        KeysCommand::Rotate { master: m } => {
            if !ctx.io.confirm("rotate the service key?")? {
                return Err(Error::Aborted.into());
            }
            let k = master(ctx, &m)?;
            let pw = ctx.password("Muninn password (wraps the new service key)")?;
            let v = keyring::rotate(&ctx.paths, &k, &pw)?;
            ctx.io.info(format!("service key rotated to v{v}"));
        }
        KeysCommand::Rekey { master: m } => {
            if !ctx.io.confirm("re-encrypt the whole database with a new data key?")? {
                return Err(Error::Aborted.into());
            }
            let k = master(ctx, &m)?;
            keyring::rekey(&ctx.paths, &k)?;
            ctx.io.info("database re-encrypted with a new data key");
        }
        KeysCommand::Export { out, only, master: m } => {
            let items: Vec<ExportItem> = if only.is_empty() {
                vec![ExportItem::Master, ExportItem::Words, ExportItem::Service, ExportItem::Shards]
            } else {
                only.iter().map(|s| ExportItem::parse(s)).collect::<crate::Result<_>>()?
            };
            let has_master = items.contains(&ExportItem::Master) || items.contains(&ExportItem::Words);
            if has_master && items.contains(&ExportItem::Shards) {
                ctx.io.warn("writing the master key/words and >= 3 shards to one directory defeats the vault/shard split");
                if !ctx.io.confirm(&format!("write all key material to {}?", out.display()))? {
                    return Err(Error::Aborted.into());
                }
            }
            let k = master(ctx, &m)?;
            let pw = ctx.export_password("Export password")?;
            if pw.expose_secret().is_empty() {
                return Err(Error::invalid("password", "must not be empty").into());
            }
            for p in keyring::export(&ctx.paths, &k, &pw, &out, &items)? {
                ctx.io.info(format!("wrote {}", p.display()));
            }
        }
        KeysCommand::Recover { master: m, print_words } => {
            let k = master(ctx, &m)?;
            let pw = new_password(ctx, "New Muninn password")?;
            keyring::recover(&ctx.paths, &k, &pw)?;
            ctx.io.info("access recovered; the service key is wrapped with the new password");
            if print_words {
                println!("{}", crypto::to_words(&k)?.as_str());
            }
        }
        KeysCommand::Shards { out, master: m } => {
            if !ctx.io.confirm("re-issue shards? the new set cannot be mixed with older shards")? {
                return Err(Error::Aborted.into());
            }
            let k = master(ctx, &m)?;
            match out {
                Some(dir) => {
                    let pw = ctx.export_password("Export password")?;
                    for p in keyring::export(&ctx.paths, &k, &pw, &dir, &[ExportItem::Shards])? {
                        ctx.io.info(format!("wrote {}", p.display()));
                    }
                }
                None => {
                    for s in crypto::split_shards(&k)? {
                        println!("{}", s.as_str());
                    }
                }
            }
        }
    }
    Ok(())
}
