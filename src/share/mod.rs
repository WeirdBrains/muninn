//! The universal Share action: anything shareable (a [`ShareKind`] item) goes to anything
//! that accepts shares (a [`Target`]): a project, a journal (as a new entry about it), a code
//! space (data is piped into it and it runs) or the network (an exchange share, `mnnn net share`).
use crate::code::pipe::Source;
use crate::error::{Error, Result};
use crate::store::{CodeInput, ItemKind, NewAction, ShareKind, Store};
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
            TargetKind::Code => "run a code space over it (the space keeps no data)",
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
            // data: a model, any live search, a journal's entries, a code space's output
            TargetKind::Code => matches!(kind, ShareKind::Model | ShareKind::LiveSearch | ShareKind::Journal | ShareKind::Code),
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
            TargetKind::Code => format!("piped {what} into code space {}", self.to),
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

/// The data a shared item stands for, as a pipe source: a model as a whole, a live search
/// (of any kind, evaluated when it runs), a journal's entries or a code space's output.
pub fn source_for(kind: ShareKind, id: &str, name: &str) -> Result<Source> {
    Ok(match kind {
        ShareKind::Model => Source::Model(CodeInput { model: name.to_string(), ..Default::default() }),
        ShareKind::LiveSearch => Source::Search(id.to_string()),
        ShareKind::Journal => Source::Journal(id.to_string()),
        ShareKind::Code => Source::Code(id.to_string()),
        _ => return Err(Error::invalid("share", format!("a {} is not data for a code space", kind.label()))),
    })
}

/// Share data into a code space: run the space over it (the space is not changed; the
/// result is the pipe's output, kept only where it is piped on).
pub fn to_code(
    s: &Store,
    env: &crate::code::CodeEnv,
    kind: ShareKind,
    key: &str,
    space: &str,
    origin: &str,
    log: &mut dyn FnMut(&str),
) -> Result<(Shared, crate::code::pipe::Piped)> {
    let out = share(s, kind, key, TargetKind::Code, &[space.to_string()], "", origin)?;
    let src = source_for(kind, &out.item, &out.name)?;
    let p = crate::code::pipe::run(s, env, origin, &src, &[crate::code::pipe::Step::Code(out.made.clone())], &[], log)?;
    Ok((out, p))
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
            // checked here; the run is `to_code` (it needs the code environment)
            let c = s.find_code_space(one()?)?;
            if kind == ShareKind::Code && c.id == id {
                return Err(Error::invalid("share", "a code space can't be piped into itself"));
            }
            source_for(kind, &id, &name)?;
            (out.to, out.made, out.new) = (c.name, c.id, false);
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
