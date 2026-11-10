//! A node for one FFI call. Kept outside `api` so the bridge does not export it.
use crate::api::data::open_write;
use crate::api::info::paths;
use crate::api::net::{hosted_node, prefs_of};
use muninn_core::net::{Node, identity, local};
use muninn_core::prefs::Preferences;
use std::sync::Arc;

pub(crate) struct HeldNode {
    owned: Option<Arc<Node>>,
    ephemeral: Option<Node>,
    pub prefs: Preferences,
    pub label: String,
}

impl HeldNode {
    pub(crate) fn node(&self) -> &Node {
        if let Some(n) = self.owned.as_deref() { n } else { self.ephemeral.as_ref().expect("a node") }
    }
}

pub(crate) fn hold_node(home: Option<String>) -> anyhow::Result<HeldNode> {
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let kp = identity::load_or_create(&store)?;
    let label = local::my_label(&store, &prefs.net);
    let owned = hosted_node();
    let ephemeral = if owned.is_some() { None } else { Some(local::ephemeral(&kp, open_write()?, &label, &prefs.net)?) };
    Ok(HeldNode { owned, ephemeral, prefs, label })
}
