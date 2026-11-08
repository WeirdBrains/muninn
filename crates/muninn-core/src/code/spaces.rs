//! Running code spaces. A space runs its main; main may call members (`host.call`) and
//! whole other spaces (`host.call_space`), declared in an `@imports` header. Only main
//! calls. A space's approval covers its line-up (main, kind, members, aliases, pins,
//! imported spaces), not sources: unpinned members run their current code, and edits since
//! a code's own approval are flagged unreviewed. Cross-space calls can't loop (checked
//! when a space is approved and at run time) and nest at most [`MAX_DEPTH`] deep.
use super::{CodeEnv, trust_hash};
use crate::error::{Error, Result};
use crate::store::{CodeSpace, NewAction, Space, SpaceKind, SpaceMember, Store};
use serde_json::{Value, json};
use std::cell::RefCell;

/// The deepest chain of spaces calling spaces.
pub const MAX_DEPTH: usize = 8;

/// What main declares it calls.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Imports {
    /// Member aliases.
    pub code: Vec<String>,
    /// Space names (`space:NAME`, or the NAME of `space:NAME as ALIAS`).
    pub spaces: Vec<String>,
    /// (the name main calls it by, the space): `space:NAME` calls NAME, and
    /// `space:NAME as ALIAS` calls ALIAS (what a renamed import keeps main calling).
    pub space_aliases: Vec<(String, String)>,
}

/// One `@imports` part: `space:NAME [as ALIAS]` as (NAME, ALIAS).
fn space_part(part: &str) -> Option<(String, Option<String>)> {
    let (k, n) = part.split_once(':')?;
    if !k.trim().eq_ignore_ascii_case("space") || n.trim().is_empty() {
        return None;
    }
    let n = n.trim();
    match n.split_once(" as ") {
        Some((t, a)) if !t.trim().is_empty() && !a.trim().is_empty() => Some((t.trim().to_string(), Some(a.trim().to_string()))),
        _ => Some((n.to_string(), None)),
    }
}

fn header(line: &str) -> Option<(usize, usize)> {
    let i = line.find("@imports")?;
    let start = i + "@imports".len();
    let rest = &line[start..];
    let trimmed = rest.trim_end();
    let body = trimmed.trim_end_matches("*/").trim_end_matches("-->").trim_end();
    Some((start, start + body.len()))
}

/// Read main's `@imports` header lines (`// @imports a, b, space:other`).
pub fn imports(source: &str) -> Imports {
    let mut out = Imports::default();
    for line in source.lines() {
        let Some((a, b)) = header(line) else { continue };
        for part in line[a..b].split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match space_part(part) {
                Some((n, alias)) => {
                    out.space_aliases.push((alias.unwrap_or_else(|| n.clone()), n.clone()));
                    out.spaces.push(n);
                }
                None => out.code.push(part.to_string()),
            }
        }
    }
    out
}

/// `source` with space `from` imported as `to` instead (`space:from` becomes
/// `space:to as from`, so main still calls it `from`); None when it doesn't import it.
pub fn rename_space_import(source: &str, from: &str, to: &str) -> Option<String> {
    let mut changed = false;
    let lines: Vec<String> = source
        .split_inclusive('\n')
        .map(|line| {
            let Some((a, b)) = header(line) else { return line.to_string() };
            let parts: Vec<String> = line[a..b]
                .split(',')
                .map(|part| match space_part(part.trim()) {
                    Some((n, alias)) if n.eq_ignore_ascii_case(from) => {
                        changed = true;
                        let alias = alias.unwrap_or(n);
                        if alias.eq_ignore_ascii_case(to) { format!(" space:{to}") } else { format!(" space:{to} as {alias}") }
                    }
                    _ => part.to_string(),
                })
                .collect();
            format!("{}{}{}", &line[..a], parts.join(","), &line[b..])
        })
        .collect();
    changed.then(|| lines.concat())
}

fn main_of(store: &Store, s: &Space) -> Result<CodeSpace> {
    store.code_space(&s.main_code_id)?.ok_or_else(|| Error::invalid("space", format!("{}: its main code is missing", s.name)))
}

/// The line-up hash: main, kind, the members (alias, code, pin) and the spaces main imports.
pub fn lineup_hash(store: &Store, s: &Space) -> Result<String> {
    let main = main_of(store, s)?;
    let mut ms: Vec<(String, String, String)> =
        store.space_members(&s.id)?.into_iter().map(|m| (m.alias.to_lowercase(), m.code_id, m.pin_hash.unwrap_or_default())).collect();
    ms.sort();
    let mut sp: Vec<String> = imports(&main.source).spaces.iter().map(|x| x.to_lowercase()).collect();
    sp.sort();
    sp.dedup();
    let mut h = blake3::Hasher::new();
    h.update(b"muninn-space-lineup\0");
    h.update(main.id.as_bytes());
    h.update(&[0]);
    h.update(s.kind.as_str().as_bytes());
    for (a, c, p) in &ms {
        h.update(format!("\0m{a}\0{c}\0{p}").as_bytes());
    }
    for x in &sp {
        h.update(format!("\0s{x}").as_bytes());
    }
    Ok(h.finalize().to_hex()[..32].to_string())
}

/// Approved for its current line-up.
pub fn approved(store: &Store, s: &Space) -> bool {
    lineup_hash(store, s).ok().is_some_and(|h| s.approved_hash.as_deref() == Some(h.as_str()))
}

/// A member's state.
#[derive(Debug, Clone)]
pub struct MemberState {
    pub member: SpaceMember,
    pub code: CodeSpace,
    pub main: bool,
    /// Pinned, and the code changed since: calls to it fail until re-pinned.
    pub pin_broken: bool,
    /// The code changed since its own approval (or was never approved): it runs anyway
    /// inside the space, flagged.
    pub unreviewed: bool,
}

pub fn members(store: &Store, s: &Space) -> Result<Vec<MemberState>> {
    let mut v = vec![];
    for m in store.space_members(&s.id)? {
        let Some(code) = store.code_space(&m.code_id)? else { continue };
        let now = trust_hash(&code).ok();
        let pin_broken = m.pin_hash.as_ref().is_some_and(|p| now.as_deref() != Some(p.as_str()));
        let unreviewed = !super::approved(&code);
        v.push(MemberState { main: code.id == s.main_code_id, member: m, code, pin_broken, unreviewed });
    }
    Ok(v)
}

/// Refuse loops through main's `space:` imports (a → b → a), and chains deeper than 8.
pub fn check_cycles(store: &Store, s: &Space) -> Result<()> {
    fn walk(store: &Store, s: &Space, path: &mut Vec<String>) -> Result<()> {
        if path.iter().any(|p| p.eq_ignore_ascii_case(&s.name)) {
            path.push(s.name.clone());
            return Err(Error::invalid("space", format!("a cycle of spaces: {}", path.join(" → "))));
        }
        if path.len() >= MAX_DEPTH {
            return Err(Error::invalid("space", format!("spaces call spaces more than {MAX_DEPTH} deep: {} → {}", path.join(" → "), s.name)));
        }
        path.push(s.name.clone());
        let main = main_of(store, s)?;
        for n in imports(&main.source).spaces {
            let t = store.find_space(&n).map_err(|_| Error::invalid("space", format!("{}: main imports space:{n}, which doesn't exist", s.name)))?;
            walk(store, &t, path)?;
        }
        path.pop();
        Ok(())
    }
    walk(store, s, &mut vec![])
}

/// The line-up is sound: declared code imports are members, imported spaces exist, are
/// callable, and don't loop.
pub fn check(store: &Store, s: &Space) -> Result<()> {
    let main = main_of(store, s)?;
    let imp = imports(&main.source);
    let ms = store.space_members(&s.id)?;
    for a in &imp.code {
        if !ms.iter().any(|m| m.alias.eq_ignore_ascii_case(a)) {
            return Err(Error::invalid(
                "space",
                format!("{}: main imports `{a}`, which is not a member (`mnnn spaces add {} CODE --as {a}`)", s.name, s.name),
            ));
        }
    }
    for n in &imp.spaces {
        let t = store.find_space(n).map_err(|_| Error::invalid("space", format!("{}: main imports space:{n}, which doesn't exist", s.name)))?;
        if t.kind == SpaceKind::IngestConnector {
            return Err(Error::invalid("space", format!("{}: space:{} is an ingest connector; connectors run only from a schedule", s.name, t.name)));
        }
    }
    check_cycles(store, s)
}

/// Approve a space's current line-up (recorded). `with_code`: also approve the current
/// source of every unreviewed member (each recorded).
pub fn approve(store: &Store, origin: &str, s: &Space, with_code: bool) -> Result<String> {
    check(store, s)?;
    if with_code {
        for m in members(store, s)? {
            if m.unreviewed {
                super::approve(store, origin, &m.code)?;
            }
        }
    }
    let h = lineup_hash(store, s)?;
    store.approve_space(&s.id, Some(&h))?;
    store.note_action(
        &NewAction::new("space.approve", origin, format!("approve code space {} (its line-up)", s.name))
            .params(json!({ "id": s.id, "name": s.name, "hash": h })),
    );
    Ok(h)
}

fn need_approval(s: &Space) -> Error {
    Error::NeedsConfirmation(format!(
        "code space {} is {}; review it (`mnnn spaces show {}`) and approve it (`mnnn spaces approve {}`)",
        s.name,
        if s.approved_hash.is_some() { "changed (its line-up) since it was approved" } else { "new" },
        s.name,
        s.name
    ))
}

/// Whether a space may run: enabled, approved, sound.
pub fn check_runnable(store: &Store, s: &Space) -> Result<()> {
    if !s.enabled {
        return Err(Error::invalid("space", format!("code space {n} is disabled; `mnnn spaces enable {n}`", n = s.name)));
    }
    if !approved(store, s) {
        return Err(need_approval(s));
    }
    check(store, s)
}

/// Everything a top-level run checks before building: [`check_runnable`] and main's pin.
/// Gives main. (Cycles and depth are checked by [`run`] itself.)
pub fn ready_main(store: &Store, s: &Space) -> Result<MemberState> {
    check_runnable(store, s)?;
    let ms = members(store, s)?;
    let main = ms.into_iter().find(|m| m.main).ok_or_else(|| Error::invalid("space", format!("{}: its main code is missing", s.name)))?;
    if main.pin_broken {
        return Err(Error::invalid(
            "space",
            format!("{}: main {} changed since pinned; `mnnn spaces pin {} {}` accepts it", s.name, main.code.name, s.name, main.code.name),
        ));
    }
    Ok(main)
}

struct Frame {
    space: Space,
    /// A member (not main) is running: it may not call.
    in_member: bool,
    /// Main is calling something now (nested runs are allowed).
    calling: bool,
}

thread_local! {
    static STACK: RefCell<Vec<Frame>> = const { RefCell::new(vec![]) };
}

/// A nested run is main calling.
pub(crate) fn nested_ok() -> bool {
    STACK.with(|s| s.borrow().iter().any(|f| f.calling))
}

struct Pop;
impl Drop for Pop {
    fn drop(&mut self) {
        STACK.with(|s| s.borrow_mut().pop());
    }
}

/// Run a space: its main over `input`. Connectors run only from a schedule.
pub fn run(
    store: &Store,
    env: &CodeEnv,
    origin: &str,
    s: &Space,
    input: Option<Value>,
    from: &str,
    log: &mut dyn FnMut(&str),
) -> Result<crate::store::RunInfo> {
    if s.kind == SpaceKind::IngestConnector {
        return Err(Error::invalid("space", format!("code space {} is an ingest connector; connectors run only from a schedule", s.name)));
    }
    run_any(store, env, origin, s, input, from, log)
}

fn run_any(
    store: &Store,
    env: &CodeEnv,
    origin: &str,
    s: &Space,
    input: Option<Value>,
    from: &str,
    log: &mut dyn FnMut(&str),
) -> Result<crate::store::RunInfo> {
    let (cycle, depth) = STACK.with(|st| {
        let st = st.borrow();
        let names: Vec<String> = st.iter().map(|f| f.space.name.clone()).collect();
        (st.iter().any(|f| f.space.id == s.id).then(|| format!("{} → {}", names.join(" → "), s.name)), st.len())
    });
    if let Some(path) = cycle {
        return Err(Error::invalid("space", format!("a cycle of spaces: {path}")));
    }
    if depth >= MAX_DEPTH {
        return Err(Error::invalid("space", format!("spaces call spaces more than {MAX_DEPTH} deep (at {})", s.name)));
    }
    let main = ready_main(store, s)?;
    if main.unreviewed {
        log(&format!("note: {} runs main {} as edited (unreviewed)", s.name, main.code.name));
    }
    STACK.with(|st| st.borrow_mut().push(Frame { space: s.clone(), in_member: false, calling: false }));
    let _pop = Pop;
    super::run_in_space(store, env, origin, &main.code, input, &format!("space:{} ({from})", s.name), log)
}

fn top_may_call() -> Result<Space> {
    STACK.with(|st| {
        let st = st.borrow();
        match st.last() {
            None => Err(Error::invalid("call", "only a code space's main calls code (run it as a space: `mnnn spaces run`)")),
            Some(f) if f.in_member => Err(Error::invalid("call", "only a space's main calls code")),
            Some(f) => Ok(f.space.clone()),
        }
    })
}

fn set_top(in_member: Option<bool>, calling: bool) {
    STACK.with(|st| {
        if let Some(f) = st.borrow_mut().last_mut() {
            if let Some(m) = in_member {
                f.in_member = m;
            }
            f.calling = calling;
        }
    });
}

struct Calling;
impl Drop for Calling {
    fn drop(&mut self) {
        set_top(Some(false), false);
    }
}

/// `host.call(alias, input)`: main runs a member over a frame; its output frame back.
pub fn host_call(store: &Store, req: &Value) -> Result<Value> {
    let s = top_may_call()?;
    let alias =
        req.get("alias").or_else(|| req.get("name")).and_then(Value::as_str).ok_or_else(|| Error::invalid("call", "give the member's alias"))?;
    let main = main_of(store, &s)?;
    if !imports(&main.source).code.iter().any(|a| a.eq_ignore_ascii_case(alias)) {
        return Err(Error::invalid("call", format!("`{alias}` is not in main's @imports (declare it: `@imports {alias}`)")));
    }
    let m = members(store, &s)?
        .into_iter()
        .find(|m| m.member.alias.eq_ignore_ascii_case(alias))
        .ok_or_else(|| Error::invalid("call", format!("`{alias}` is not a member of {}", s.name)))?;
    if m.pin_broken {
        return Err(Error::invalid("call", format!("{} changed since pinned; `mnnn spaces pin {} {}` accepts it", m.code.name, s.name, m.code.name)));
    }
    let input = req.get("input").cloned().filter(|v| !v.is_null());
    let env = CodeEnv::for_store(store)?;
    set_top(Some(true), true);
    let _c = Calling;
    let info = super::run_in_space(store, &env, "code", &m.code, input, &format!("space:{} main", s.name), &mut |_| {})?;
    super::pipe::output(&m.code, &info)
}

/// `host.call_space(name, input)`: main runs another space (its main); its output back.
pub fn host_call_space(store: &Store, req: &Value) -> Result<Value> {
    let s = top_may_call()?;
    let name = req.get("name").and_then(Value::as_str).ok_or_else(|| Error::invalid("call_space", "give the space's name"))?;
    let main = main_of(store, &s)?;
    let Some((_, target)) = imports(&main.source).space_aliases.into_iter().find(|(a, _)| a.eq_ignore_ascii_case(name)) else {
        return Err(Error::invalid("call_space", format!("space:{name} is not in main's @imports (declare it: `@imports space:{name}`)")));
    };
    let t = store.find_space(&target)?;
    if t.kind == SpaceKind::IngestConnector {
        return Err(Error::invalid("call_space", format!("{} is an ingest connector; connectors run only from a schedule", t.name)));
    }
    let input = req.get("input").cloned().filter(|v| !v.is_null());
    let env = CodeEnv::for_store(store)?;
    set_top(None, true);
    let _c = Calling;
    let info = run_any(store, &env, "code", &t, input, &format!("space:{} main", s.name), &mut |_| {})?;
    super::pipe::output(&main_of(store, &t)?, &info)
}

/// Create a space (recorded). Not approved yet.
pub fn create(store: &Store, origin: &str, name: &str, description: &str, kind: SpaceKind, main: &CodeSpace) -> Result<Space> {
    let s = store.create_space(name, description, kind, main)?;
    store.note_action(
        &NewAction::new("space.create", origin, format!("create code space {} ({kind}, main {})", s.name, main.name))
            .params(json!({ "id": s.id, "name": s.name, "kind": kind, "main": main.id })),
    );
    Ok(s)
}

/// Record a change to a space (members, main, pins, kind…).
pub fn note(store: &Store, origin: &str, s: &Space, what: &str, params: Value) {
    store.note_action(
        &NewAction::new("space.edit", origin, format!("code space {}: {what}", s.name))
            .params(json!({ "id": s.id, "name": s.name, "change": params })),
    );
}

/// Remove a space (its code stays; recorded).
pub fn remove(store: &Store, origin: &str, id: &str) -> Result<Space> {
    let s = store.remove_space(id)?;
    store.note_action(
        &NewAction::new("space.remove", origin, format!("remove code space {}", s.name))
            .params(json!({ "id": s.id, "name": s.name, "main": s.main_code_id })),
    );
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{CodeChange, CodeInput, Lang};

    fn edit(s: &Store, id: &str, src: &str) {
        s.update_code_space(id, &CodeChange { source: Some(src.into()), ..Default::default() }).unwrap();
    }

    #[test]
    fn imports_header() {
        let i = imports("// @imports parse-csv, space:cleaner , to-bars\nfn run() {}\n# @imports more");
        assert_eq!(i.code, vec!["parse-csv", "to-bars", "more"]);
        assert_eq!(i.spaces, vec!["cleaner"]);
        assert_eq!(imports("/* @imports a */").code, vec!["a"]);
    }

    #[test]
    fn lineup_approval_follows_edits_but_not_lineup_changes() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_code_space("a", "", Lang::Math, "x = close", CodeInput::default()).unwrap();
        let b = s.create_code_space("b", "", Lang::Math, "y = close", CodeInput::default()).unwrap();
        let sp = create(&s, "test", "sp", "", SpaceKind::Analysis, &a).unwrap();
        assert!(!approved(&s, &sp));
        approve(&s, "test", &sp, false).unwrap();
        let sp = s.space(&sp.id).unwrap().unwrap();
        assert!(approved(&s, &sp));
        edit(&s, &a.id, "x = open");
        assert!(approved(&s, &sp), "an edit keeps the space approved");
        assert!(members(&s, &sp).unwrap()[0].unreviewed, "and is flagged");
        s.add_space_member(&sp.id, &b, None, None).unwrap();
        assert!(!approved(&s, &sp), "a line-up change needs approving again");
        approve(&s, "test", &sp, true).unwrap();
        let sp = s.space(&sp.id).unwrap().unwrap();
        assert!(approved(&s, &sp) && members(&s, &sp).unwrap().iter().all(|m| !m.unreviewed));
        let h = trust_hash(&s.code_space(&b.id).unwrap().unwrap()).unwrap();
        s.set_member_pin(&sp.id, &b.id, Some(&h)).unwrap();
        assert!(!approved(&s, &sp), "a pin is part of the line-up");
        edit(&s, &b.id, "y = 2");
        assert!(members(&s, &sp).unwrap().iter().any(|m| m.pin_broken));
        let before = lineup_hash(&s, &sp).unwrap();
        edit(&s, &a.id, "# @imports space:other\nx = close");
        assert_ne!(lineup_hash(&s, &sp).unwrap(), before, "main's space imports are part of the line-up");
    }

    #[test]
    fn space_cycles_and_imports_checked() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_code_space("a", "", Lang::Math, "# @imports space:sb\nx = close", CodeInput::default()).unwrap();
        let b = s.create_code_space("b", "", Lang::Math, "# @imports space:sa\ny = close", CodeInput::default()).unwrap();
        let sa = create(&s, "t", "sa", "", SpaceKind::Analysis, &a).unwrap();
        let sb = create(&s, "t", "sb", "", SpaceKind::Analysis, &b).unwrap();
        let e = approve(&s, "t", &sa, false).unwrap_err().to_string();
        assert!(e.contains("sa → sb → sa"), "{e}");
        edit(&s, &b.id, "# @imports nope\ny = 1");
        assert!(approve(&s, "t", &sb, false).unwrap_err().to_string().contains("not a member"));
        let mut prev: Option<String> = None;
        let mut last = None;
        for i in 0..10 {
            let src = match &prev {
                Some(p) => format!("# @imports space:{p}\nx = close"),
                None => "x = close".into(),
            };
            let c = s.create_code_space(&format!("c{i}"), "", Lang::Math, &src, CodeInput::default()).unwrap();
            let sp = create(&s, "t", &format!("d{i}"), "", SpaceKind::Analysis, &c).unwrap();
            prev = Some(sp.name.clone());
            last = Some(sp);
        }
        assert!(check_cycles(&s, &last.unwrap()).unwrap_err().to_string().contains("deep"));
    }
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn space_imports_take_aliases_and_follow_renames() {
        let i = imports("// @imports dbl, space:a, space:b as bee\nx = 1\n");
        assert_eq!(i.code, ["dbl"]);
        assert_eq!(i.spaces, ["a", "b"]);
        assert_eq!(i.space_aliases, [("a".to_string(), "a".to_string()), ("bee".to_string(), "b".to_string())]);
        let src = "/* @imports space:a, dbl */\nx = 1\n";
        let r = rename_space_import(src, "a", "a-imported").unwrap();
        assert_eq!(r, "/* @imports space:a-imported as a, dbl */\nx = 1\n");
        assert_eq!(imports(&r).space_aliases, [("a".to_string(), "a-imported".to_string())]);
        // an alias is kept; renaming back to the alias drops it; others are untouched
        assert_eq!(rename_space_import("// @imports space:b as bee", "b", "b2").unwrap(), "// @imports space:b2 as bee");
        assert_eq!(rename_space_import("// @imports space:b as bee", "b", "bee").unwrap(), "// @imports space:bee");
        assert!(rename_space_import(src, "zzz", "y").is_none());
    }
}
