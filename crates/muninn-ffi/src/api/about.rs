//! About Muninn, from `muninn_core::about` (the TUI shows the same text).
use muninn_core::about as a;

pub struct AboutInfo {
    pub name: String,
    pub version: String,
    pub tagline: String,
    pub motto: String,
    pub publisher: String,
    pub homepage: String,
    pub repository: String,
    pub license: String,
    pub copyright: String,
    pub credits: Vec<String>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn about() -> AboutInfo {
    AboutInfo {
        name: a::NAME.into(),
        version: a::VERSION.into(),
        tagline: a::TAGLINE.into(),
        motto: a::MOTTO.into(),
        publisher: a::PUBLISHER.into(),
        homepage: a::HOMEPAGE.into(),
        repository: a::REPOSITORY.into(),
        license: a::LICENSE.into(),
        copyright: a::COPYRIGHT.into(),
        credits: a::CREDITS.iter().map(|s| s.to_string()).collect(),
    }
}
