//! The API the Flutter app calls (generated bindings: `app/lib/src/rust/`).
//! Thin wrappers over `muninn-core`; no logic beyond shaping data for Dart.
pub mod about;
pub mod data;
pub mod info;
pub mod prefs;
pub mod themes;

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
