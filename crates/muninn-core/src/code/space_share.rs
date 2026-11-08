//! Code space files: a space with its main, its members and the spaces its main imports,
//! every member's code by trust hash (as pipeline files carry them). What every share path
//! (files, the network, orgs, project packs) moves for a space. Imports land unapproved, pins
//! kept; the same line-up is reused, a different one lands as `NAME-imported`.
use crate::code::pipeline::share::{CodeEntry, SpaceEntry, bundle_spaces, land_code, land_spaces, plan_code, plan_spaces};
use crate::code::trust_hash_of;
use crate::error::{Error, Result};
use crate::store::{Space, Store};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "muninn-space";
pub const VERSION: u32 = 1;

/// A code space file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    /// The space shared (first in `spaces`); the rest are the spaces its main imports.
    pub name: String,
    pub spaces: Vec<SpaceEntry>,
    pub code: Vec<CodeEntry>,
}

impl Bundle {
    pub fn parse(bytes: &[u8]) -> Result<Bundle> {
        let b: Bundle = serde_json::from_slice(bytes).map_err(|e| Error::invalid("code space", format!("not a code space file ({e})")))?;
        if b.format != FORMAT {
            return Err(Error::invalid("code space", format!("format `{}`: expected {FORMAT}", b.format)));
        }
        if b.version > VERSION {
            return Err(Error::invalid("code space", format!("version {} is newer than this muninn reads ({VERSION})", b.version)));
        }
        for c in &b.code {
            if trust_hash_of(c.lang, &c.source)? != c.hash {
                return Err(Error::invalid("code space", format!("code {}: its source does not match its hash", c.name)));
            }
        }
        if !b.spaces.iter().any(|s| s.name == b.name) {
            return Err(Error::invalid("code space", format!("the file has no space {}", b.name)));
        }
        for s in &b.spaces {
            for c in std::iter::once(&s.main).chain(s.members.iter().map(|m| &m.code)) {
                if !b.code.iter().any(|x| &x.name == c) {
                    return Err(Error::invalid("code space", format!("{}: its code {c} is not in the file", s.name)));
                }
            }
        }
        Ok(b)
    }
}

/// Bundle space `key` (name or id).
pub fn export(store: &Store, key: &str) -> Result<Bundle> {
    let s = store.find_space(key)?;
    let mut code = vec![];
    let mut spaces = bundle_spaces(store, vec![s.name.clone()], &mut code)?;
    // the shared space first
    if let Some(i) = spaces.iter().position(|x| x.name == s.name) {
        let root = spaces.remove(i);
        spaces.insert(0, root);
    }
    Ok(Bundle { format: FORMAT.into(), version: VERSION, name: s.name, spaces, code })
}

/// What importing would do.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Plan {
    /// (code in the file, code it lands as, what happens)
    pub code: Vec<(String, String, String)>,
    /// (space in the file, space it lands as, what happens)
    pub spaces: Vec<(String, String, String)>,
    pub problems: Vec<String>,
}

impl Plan {
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![];
        v.extend(self.spaces.iter().map(|(a, b, w)| if a == b { format!("space {a}: {w}") } else { format!("space {a} → {b}: {w}") }));
        v.extend(self.code.iter().map(|(a, b, w)| if a == b { format!("  code {a}: {w}") } else { format!("  code {a} → {b}: {w}") }));
        v.extend(self.problems.iter().map(|p| format!("  ! {p}")));
        v
    }
    /// The name the shared space lands as.
    pub fn landed(&self, b: &Bundle) -> String {
        self.spaces.iter().find(|(a, _, _)| *a == b.name).map(|(_, t, _)| t.clone()).unwrap_or_else(|| b.name.clone())
    }
}

/// The plan; `name` lands the shared space under another name.
pub fn plan(store: &Store, b: &Bundle, name: Option<&str>) -> Result<Plan> {
    let code = plan_code(store, &b.code)?;
    let mut spaces = plan_spaces(store, &b.spaces, &code)?;
    if let Some(n) = name.map(str::trim).filter(|n| !n.is_empty()) {
        crate::store::spaces::check_name(n)?;
        if store.space_name_taken(n)? {
            return Err(Error::invalid("code space", format!("`{n}` exists already")));
        }
        if let Some(s) = spaces.iter_mut().find(|(a, _, _)| *a == b.name) {
            (s.1, s.2) = (n.to_string(), "created under this name; needs approval before it runs".into());
        }
    }
    let mut problems = vec![];
    let mut code = code;
    // a rewritten main changes its space's line-up, which may rename that space too
    for _ in 0..crate::code::spaces::MAX_DEPTH {
        let Some(r) = rewritten(b, &spaces)? else { break };
        code = plan_code(store, &r.0.code)?;
        let mut again = plan_spaces(store, &r.0.spaces, &code)?;
        // keep the name the shared space was given
        if let (Some(x), Some(given)) = (again.iter_mut().find(|(x, _, _)| *x == b.name), spaces.iter().find(|(x, _, _)| *x == b.name))
            && name.is_some()
        {
            *x = given.clone();
        }
        if again == spaces {
            problems.extend(r.1.iter().map(|(main, imports)| format!("main {main}: @imports rewritten ({})", imports.join(", "))));
            break;
        }
        spaces = again;
    }
    Ok(Plan { code, spaces, problems })
}

/// Per main code, what its `@imports` rewrite changed.
pub type Rewrites = Vec<(String, Vec<String>)>;

/// The bundle with its mains' `@imports` following spaces that land under another name
/// (`space:A` → `space:A-imported as A`, so main's calls are unchanged; pinned members
/// pin the rewritten code), and per main what changed. None when nothing is renamed.
pub fn rewritten(b: &Bundle, spaces: &[(String, String, String)]) -> Result<Option<(Bundle, Rewrites)>> {
    let renames: Vec<(&String, &String)> = spaces.iter().filter(|(a, t, _)| a != t).map(|(a, t, _)| (a, t)).collect();
    if renames.is_empty() {
        return Ok(None);
    }
    let mut out = b.clone();
    let mut notes: Vec<(String, Vec<String>)> = vec![];
    let mains: std::collections::BTreeSet<String> = b.spaces.iter().map(|s| s.main.clone()).collect();
    for c in out.code.iter_mut().filter(|c| mains.contains(&c.name)) {
        let mut src = c.source.clone();
        let mut what = vec![];
        for (from, to) in &renames {
            if let Some(s) = crate::code::spaces::rename_space_import(&src, from, to) {
                src = s;
                what.push(format!("space:{from} → space:{to} as {from}"));
            }
        }
        if !what.is_empty() {
            c.hash = trust_hash_of(c.lang, &src)?;
            c.source = src;
            notes.push((c.name.clone(), what));
        }
    }
    Ok(if notes.is_empty() { None } else { Some((out, notes)) })
}

/// Import: code and spaces created unapproved (pins kept); returns the shared space.
pub fn import(store: &Store, b: &Bundle, plan: &Plan) -> Result<Space> {
    let r = rewritten(b, &plan.spaces)?;
    let b2 = r.as_ref().map(|r| &r.0).unwrap_or(b);
    land_code(store, &b2.code, &plan.code)?;
    land_spaces(store, &b2.spaces, &plan.code, &plan.spaces)?;
    store.find_space(&plan.landed(b))
}

/// Parse, plan and import in one go (peers, orgs): the shared space and the plan's lines.
pub fn receive(store: &Store, bytes: &[u8]) -> Result<(Space, Vec<String>)> {
    let b = Bundle::parse(bytes)?;
    let p = plan(store, &b, None)?;
    Ok((import(store, &b, &p)?, p.lines()))
}
