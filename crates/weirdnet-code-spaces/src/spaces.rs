//! Spaces: a main code space plus members it calls and other spaces it imports, declared
//! in an `@imports` header (`// @imports a, b, space:other`). This module has the parts that
//! don't need the application's storage: reading and rewriting the header, the line-up
//! hash an owner approves, and the cycle and depth check over a [`Graph`] the application
//! provides.
use crate::Profile;
use crate::error::SpacesError;

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

/// What a space's approval covers: main, the kind, the members (alias, code id, pin) and
/// the spaces main imports. Aliases and imported names are compared without case.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lineup<'a> {
    pub main_id: &'a str,
    pub kind: &'a str,
    /// (alias, code id, pin hash or "").
    pub members: Vec<(&'a str, &'a str, &'a str)>,
    /// Main's source, for its `space:` imports.
    pub main_source: &'a str,
}

/// The line-up hash under a profile (its `lineup_domain`).
pub fn lineup_hash(profile: &Profile, l: &Lineup) -> String {
    let mut ms: Vec<(String, String, String)> = l.members.iter().map(|(a, c, p)| (a.to_lowercase(), c.to_string(), p.to_string())).collect();
    ms.sort();
    let mut sp: Vec<String> = imports(l.main_source).spaces.iter().map(|x| x.to_lowercase()).collect();
    sp.sort();
    sp.dedup();
    let mut h = blake3::Hasher::new();
    h.update(profile.lineup_domain);
    h.update(l.main_id.as_bytes());
    h.update(&[0]);
    h.update(l.kind.as_bytes());
    for (a, c, p) in &ms {
        h.update(format!("\0m{a}\0{c}\0{p}").as_bytes());
    }
    for x in &sp {
        h.update(format!("\0s{x}").as_bytes());
    }
    h.finalize().to_hex()[..32].to_string()
}

/// The spaces as the application stores them, for [`check_cycles`].
pub trait Graph {
    type Space;
    type Error: From<SpacesError>;
    fn name<'s>(&self, s: &'s Self::Space) -> &'s str;
    /// The source of the space's main code.
    fn main_source(&self, s: &Self::Space) -> Result<String, Self::Error>;
    /// The space called `name`, if there is one.
    fn find(&self, name: &str) -> Option<Self::Space>;
}

/// Refuse loops through main's `space:` imports (a → b → a), and chains deeper than
/// [`MAX_DEPTH`].
pub fn check_cycles<G: Graph>(g: &G, s: &G::Space) -> Result<(), G::Error> {
    fn walk<G: Graph>(g: &G, s: &G::Space, path: &mut Vec<String>) -> Result<(), G::Error> {
        let name = g.name(s);
        if path.iter().any(|p| p.eq_ignore_ascii_case(name)) {
            path.push(name.to_string());
            return Err(SpacesError::invalid("space", format!("a cycle of spaces: {}", path.join(" → "))).into());
        }
        if path.len() >= MAX_DEPTH {
            return Err(
                SpacesError::invalid("space", format!("spaces call spaces more than {MAX_DEPTH} deep: {} → {}", path.join(" → "), name)).into()
            );
        }
        path.push(name.to_string());
        for n in imports(&g.main_source(s)?).spaces {
            let t = g.find(&n).ok_or_else(|| SpacesError::invalid("space", format!("{}: main imports space:{n}, which doesn't exist", g.name(s))))?;
            walk(g, &t, path)?;
        }
        path.pop();
        Ok(())
    }
    walk(g, s, &mut vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn imports_and_renames() {
        let i = imports("// @imports fast, slow, space:prices as p\n/* @imports space:other */\nfn run() {}");
        assert_eq!(i.code, vec!["fast", "slow"]);
        assert_eq!(i.spaces, vec!["prices", "other"]);
        assert_eq!(i.space_aliases, vec![("p".to_string(), "prices".to_string()), ("other".to_string(), "other".to_string())]);
        let r = rename_space_import("// @imports a, space:old\nx\n", "old", "new").unwrap();
        assert_eq!(r, "// @imports a, space:new as old\nx\n");
        assert_eq!(imports(&r).space_aliases, vec![("old".to_string(), "new".to_string())]);
        assert!(rename_space_import("// @imports a\n", "old", "new").is_none());
    }

    #[test]
    fn lineup_hash_ignores_case_and_order() {
        let a = Lineup {
            main_id: "m",
            kind: "analysis",
            members: vec![("A", "c1", ""), ("b", "c2", "pin")],
            main_source: "// @imports space:X, space:y",
        };
        let b = Lineup {
            main_id: "m",
            kind: "analysis",
            members: vec![("B", "c2", "pin"), ("a", "c1", "")],
            main_source: "// @imports space:Y, space:x, space:x",
        };
        assert_eq!(lineup_hash(&crate::WEIRDNET, &a), lineup_hash(&crate::WEIRDNET, &b));
        let c = Lineup { kind: "transform", ..a.clone() };
        assert_ne!(lineup_hash(&crate::WEIRDNET, &a), lineup_hash(&crate::WEIRDNET, &c));
        assert_eq!(lineup_hash(&crate::WEIRDNET, &a).len(), 32);
    }

    struct G(HashMap<&'static str, &'static str>);
    impl Graph for G {
        type Space = String;
        type Error = SpacesError;
        fn name<'s>(&self, s: &'s String) -> &'s str {
            s
        }
        fn main_source(&self, s: &String) -> Result<String, SpacesError> {
            Ok(self.0[s.as_str()].to_string())
        }
        fn find(&self, name: &str) -> Option<String> {
            let n = name.to_lowercase();
            self.0.contains_key(n.as_str()).then_some(n)
        }
    }

    #[test]
    fn cycles_depth_and_missing_imports() {
        let g = G(HashMap::from([("a", "// @imports space:b"), ("b", "// @imports space:A"), ("c", "// @imports space:nope"), ("d", "")]));
        let e = check_cycles(&g, &"a".to_string()).unwrap_err().to_string();
        assert_eq!(e, "invalid space: a cycle of spaces: a → b → a");
        let e = check_cycles(&g, &"c".to_string()).unwrap_err().to_string();
        assert_eq!(e, "invalid space: c: main imports space:nope, which doesn't exist");
        check_cycles(&g, &"d".to_string()).unwrap();
        let names = ["s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9"];
        let srcs = [
            "// @imports space:s1",
            "// @imports space:s2",
            "// @imports space:s3",
            "// @imports space:s4",
            "// @imports space:s5",
            "// @imports space:s6",
            "// @imports space:s7",
            "// @imports space:s8",
            "// @imports space:s9",
            "",
        ];
        let g = G(names.iter().copied().zip(srcs).collect());
        let e = check_cycles(&g, &"s0".to_string()).unwrap_err().to_string();
        assert!(e.contains("more than 8 deep: s0 → s1 → s2 → s3 → s4 → s5 → s6 → s7 → s8"), "{e}");
        check_cycles(&g, &"s2".to_string()).unwrap();
    }
}
