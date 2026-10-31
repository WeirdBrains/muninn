//! The universal Share action: anything shareable (a [`ShareKind`] item) goes to anything
//! that accepts shares (a [`Target`]): a project, a journal (as a new entry about it), a code
//! space (a model becomes its input) or the network (an exchange share, `mnnn net share`).
use crate::error::{Error, Result};
use crate::store::{CodeChange, CodeInput, ItemKind, NewAction, SearchTarget, ShareKind, Store};
use serde::Serialize;

/// The kinds of place a share can go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Project,
    Journal,
    Code,
    Net,
}

impl TargetKind {
    pub const ALL: [TargetKind; 4] = [TargetKind::Project, TargetKind::Journal, TargetKind::Code, TargetKind::Net];
    pub fn as_str(self) -> &'static str {
        match self {
            TargetKind::Project => "project",
            TargetKind::Journal => "journal",
            TargetKind::Code => "code",
            TargetKind::Net => "net",
        }
    }
    /// What sharing there does, for menus and help.
    pub fn verb(self) -> &'static str {
        match self {
            TargetKind::Project => "add it to a project",
            TargetKind::Journal => "write a new journal entry about it",
            TargetKind::Code => "make it a code space's input (and run it)",
            TargetKind::Net => "share it with your exchanges",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "project" | "projects" => TargetKind::Project,
            "journal" | "journals" | "entry" => TargetKind::Journal,
            "code" | "code_space" | "code-space" => TargetKind::Code,
            "net" | "network" | "exchange" | "exchanges" => TargetKind::Net,
            _ => return None,
        })
    }
    /// Whether this kind of target takes this kind of item.
    pub fn accepts(self, kind: ShareKind) -> bool {
        match self {
            TargetKind::Project => kind != ShareKind::Project,
            TargetKind::Journal | TargetKind::Net => true,
            // data: a model, or a live search (its model and conditions)
            TargetKind::Code => matches!(kind, ShareKind::Model | ShareKind::LiveSearch),
        }
    }
    /// Whether a target of this kind is named (`net` takes exchanges, or none for all).
    pub fn needs_key(self) -> bool {
        self != TargetKind::Net
    }
}

/// Where `kind` items can be shared.
pub fn targets_for(kind: ShareKind) -> Vec<TargetKind> {
    TargetKind::ALL.into_iter().filter(|t| t.accepts(kind)).collect()
}

/// A share's outcome.
#[derive(Debug, Clone, Serialize)]
pub struct Shared {
    pub kind: ShareKind,
    pub item: String,
    pub name: String,
    pub target: TargetKind,
    /// The target's name (or the exchanges for `net`).
    pub to: String,
    /// What was made or changed: a project item, entry or share id, or the code space.
    pub made: String,
    /// False when it was already there.
    pub new: bool,
}

impl Shared {
    pub fn summary(&self) -> String {
        let what = format!("{} {}", self.kind.label(), self.name);
        match self.target {
            TargetKind::Project if !self.new => format!("{what} is already in project {}", self.to),
            TargetKind::Project => format!("added {what} to project {}", self.to),
            TargetKind::Journal => format!("wrote an entry about {what} in journal {}", self.to),
            TargetKind::Code => format!("code space {} now reads {what}", self.to),
            TargetKind::Net => format!("shared {what} with {}", self.to),
        }
    }
}

fn item_kind(k: ShareKind) -> Option<ItemKind> {
    Some(match k {
        ShareKind::Model => ItemKind::Model,
        ShareKind::LiveSearch => ItemKind::LiveSearch,
        ShareKind::Journal => ItemKind::Journal,
        ShareKind::Report => ItemKind::Report,
        ShareKind::Code => ItemKind::Code,
        ShareKind::Project => return None,
    })
}

/// The Markdown of a journal entry about a shared item.
pub fn entry_body(kind: ShareKind, id: &str, name: &str, note: &str) -> String {
    let mut b = format!("Shared {} **{name}** (`{}`, id `{id}`).\n", kind.label(), kind.as_str());
    if !note.trim().is_empty() {
        b.push('\n');
        b.push_str(note.trim());
        b.push('\n');
    }
    b
}

/// The code space input a shared item stands for: a model as a whole, or a live search's
/// records (its model, conditions and text).
pub fn input_for(s: &Store, kind: ShareKind, id: &str, name: &str) -> Result<CodeInput> {
    match kind {
        ShareKind::Model => Ok(CodeInput { model: name.to_string(), ..Default::default() }),
        ShareKind::LiveSearch => {
            let ls = s.find_live_search(id)?;
            let sp = &ls.spec;
            let model = match (sp.target, &sp.model) {
                (SearchTarget::Records, Some(m)) => s.model(m)?.map(|m| m.name).unwrap_or_else(|| m.clone()),
                _ => {
                    return Err(Error::invalid(
                        "share",
                        format!(
                            "live search {} is not over one model's records (save it with --model), so it can't be a code space's input",
                            ls.name
                        ),
                    ));
                }
            };
            Ok(CodeInput {
                model,
                filter: sp.conditions.iter().map(crate::rules::condition_text).collect(),
                text: sp.query.trim().to_string(),
                ..Default::default()
            })
        }
        _ => Err(Error::invalid("share", format!("a {} is not data for a code space", kind.label()))),
    }
}

/// Share item `key` of `kind` to `target` (`to`: its id or name; for `net`, exchange
/// labels/ids, empty for every exchange). `note` goes into journal entries.
pub fn share(s: &Store, kind: ShareKind, key: &str, target: TargetKind, to: &[String], note: &str, origin: &str) -> Result<Shared> {
    if !target.accepts(kind) {
        return Err(Error::invalid(
            "share",
            format!(
                "a {} can't take a {}; it can go to: {}",
                target.as_str(),
                kind.label(),
                targets_for(kind).iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    let one = || -> Result<&str> {
        match to {
            [k] => Ok(k.as_str()),
            [] => Err(Error::invalid("share", format!("name the {} to share to", target.as_str()))),
            _ => Err(Error::invalid("share", format!("one {} at a time", target.as_str()))),
        }
    };
    let (id, name) = crate::net::client::find_item(s, kind, key)?;
    let mut out = Shared { kind, item: id.clone(), name: name.clone(), target, to: String::new(), made: String::new(), new: true };
    match target {
        TargetKind::Project => {
            let p = s.find_project(one()?)?;
            let ik = item_kind(kind).expect("accepts() keeps projects out");
            let (it, new) = s.add_project_item(&p.id, ik, &id, serde_json::json!({}))?;
            (out.to, out.made, out.new) = (p.name, it.id.to_string(), new);
        }
        TargetKind::Journal => {
            let j = s.find_journal(one()?)?;
            if kind == ShareKind::Journal && j.id == id {
                return Err(Error::invalid("share", "a journal can't be shared into itself"));
            }
            let e = s.add_entry(&j.id, &format!("Shared: {name}"), &entry_body(kind, &id, &name, note), None)?;
            (out.to, out.made) = (j.name, e.id);
        }
        TargetKind::Code => {
            let c = s.find_code_space(one()?)?;
            let input = input_for(s, kind, &id, &name)?;
            out.new = input != c.input;
            let c = s.update_code_space(&c.id, &CodeChange { input: Some(input), ..Default::default() })?;
            (out.to, out.made) = (c.name, c.id);
        }
        TargetKind::Net => {
            let made = crate::net::client::share(s, kind, &id, to, origin)?;
            out.to = if to.is_empty() { "everyone with an exchange".into() } else { to.join(", ") };
            out.made = made.iter().map(|(sh, _)| sh.id.clone()).collect::<Vec<_>>().join(",");
            out.new = made.iter().any(|(_, n)| *n);
            return Ok(out); // net::client::share records its own history
        }
    }
    s.note_action(&NewAction::new("share", origin, out.summary()).params(serde_json::json!({
        "kind": kind, "item": id, "target": target, "to": out.to, "made": out.made,
    })));
    Ok(out)
}

#[cfg(test)]
mod tests;
