//! The API the Flutter app calls (generated bindings: `app/lib/src/rust/`).
//! Thin wrappers over `muninn-core`; no logic beyond shaping data for Dart.
pub mod about;
pub mod authors;
pub mod code;
pub mod collab;
pub mod data;
pub mod help;
pub mod history;
pub mod info;
pub mod io;
pub mod journals;
pub mod keys;
pub mod layouts;
pub mod mcp;
pub mod models;
pub mod net;
pub mod notify;
pub mod orgs;
pub mod prefs;
pub mod projects;
pub mod reports;
pub mod reviews;
pub mod rules;
pub mod schedules;
pub mod searches;
pub mod themes;

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
