//! The API the Flutter app calls (generated bindings: `app/lib/src/rust/`).
//! Thin wrappers over `muninn-core`; no logic beyond shaping data for Dart.
pub mod about;
pub mod code;
pub mod data;
pub mod help;
pub mod info;
pub mod io;
pub mod journals;
pub mod keys;
pub mod net;
pub mod orgs;
pub mod prefs;
pub mod reports;
pub mod rules;
pub mod reviews;
pub mod notify;
pub mod projects;
pub mod searches;
pub mod themes;

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
