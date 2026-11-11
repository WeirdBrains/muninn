use super::*;

/// `sdk/` is a copy of the `weirdnet-plugin` crate (it must ship inside this package);
/// in the repository the two must not drift.
#[test]
fn the_sdk_copy_matches_the_weirdnet_plugin_crate() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../weirdnet-plugin");
    if !crate_dir.join("Cargo.toml").exists() {
        return; // a packaged copy of this crate: nothing to compare with
    }
    for (n, c) in WEIRDNET_SDK_FILES {
        let theirs = std::fs::read_to_string(crate_dir.join(n)).unwrap();
        assert_eq!(&theirs, c, "sdk/{n} differs from crates/weirdnet-plugin/{n} (copy it over)");
    }
    let mut src: Vec<String> =
        std::fs::read_dir(crate_dir.join("src")).unwrap().map(|e| format!("src/{}", e.unwrap().file_name().to_string_lossy())).collect();
    src.sort();
    let mut ours: Vec<String> = WEIRDNET_SDK_FILES.iter().map(|(n, _)| n.to_string()).filter(|n| n.starts_with("src/")).collect();
    ours.sort();
    assert_eq!(src, ours, "every SDK source file is embedded");
}

#[test]
fn the_profile_matches_the_sdk() {
    let lib = WEIRDNET_SDK_FILES.iter().find(|(n, _)| *n == "src/lib.rs").unwrap().1;
    for s in WEIRDNET.symbols {
        assert!(lib.contains(&format!("fn {s}(")), "the SDK exports {s}");
    }
    assert_eq!(WEIRDNET.abi_version, 2);
    let toml = WEIRDNET_SDK_FILES[0].1;
    assert!(toml.contains(&format!("name = \"{}\"", WEIRDNET.sdk_package)));
    assert!(toml.contains(&format!("version = \"{}\"", WEIRDNET.sdk_version)));
    assert!(toml.contains("\n[workspace]\n"), "the SDK builds outside any workspace");
    assert!(lib.contains(&format!("\\\"sdk\\\":\\\"{}\\\"", WEIRDNET.sdk_version)), "plugin info names the SDK version");
}

#[test]
fn trust_hashes_cover_domain_language_and_source() {
    let a = trust_hash_of(&WEIRDNET, Lang::Math, "x = close").unwrap();
    assert_eq!(a.len(), 32);
    assert_ne!(a, trust_hash_of(&WEIRDNET, Lang::Math, "x = close ").unwrap());
    assert_ne!(a, trust_hash_of(&WEIRDNET, Lang::Rust, "x = close").unwrap());
    let other = Profile { trust_domain: b"other-code-trust\0", ..WEIRDNET };
    assert_ne!(a, trust_hash_of(&other, Lang::Math, "x = close").unwrap());
    assert!(approved(&WEIRDNET, Lang::Math, "x = close", Some(&a)));
    assert!(!approved(&WEIRDNET, Lang::Math, "x = close2", Some(&a)));
    assert!(!approved(&WEIRDNET, Lang::Math, "x = close", None));
    assert!(!approved(&other, Lang::Math, "x = close", Some(&a)), "an approval is only good under its profile");

    // a prebuilt library is trusted by its bytes, not its path
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("lib.so");
    std::fs::write(&f, b"one").unwrap();
    let src = f.display().to_string();
    let h1 = trust_hash_of(&WEIRDNET, Lang::Plugin, &src).unwrap();
    assert!(approved(&WEIRDNET, Lang::Plugin, &src, Some(&h1)));
    std::fs::write(&f, b"two").unwrap();
    assert!(!approved(&WEIRDNET, Lang::Plugin, &src, Some(&h1)), "a replaced library needs approving again");
    std::fs::remove_file(&f).unwrap();
    assert!(trust_hash_of(&WEIRDNET, Lang::Plugin, &src).unwrap_err().to_string().starts_with("invalid plugin: "));
    assert!(!approved(&WEIRDNET, Lang::Plugin, &src, Some(&h1)));
}

#[test]
fn build_keys_follow_the_profile() {
    let k = build_key(&WEIRDNET, Lang::Math, "x = close");
    assert_eq!(k, build_key(&WEIRDNET, Lang::Math, "x = close"));
    assert_ne!(k, build_key(&WEIRDNET, Lang::Rust, "x = close"));
    assert_ne!(k, build_key(&Profile { codegen: 2, ..WEIRDNET }, Lang::Math, "x = close"));
    assert_ne!(k, build_key(&Profile { abi_version: 1, ..WEIRDNET }, Lang::Math, "x = close"));
    assert_ne!(sdk_hash(&WEIRDNET), sdk_hash(&Profile { sdk_files: &[("Cargo.toml", "")], ..WEIRDNET }));
}

#[test]
fn generated_crates_name_the_profile_sdk() {
    let d = tempfile::tempdir().unwrap();
    let env = Env::new(d.path().join("code"), d.path().join("target"));
    assert!(env.sdk_dir(&WEIRDNET).ends_with(format!("sdk/weirdnet-plugin-{}", &sdk_hash(&WEIRDNET)[..12])));
    let dir = env.build_dir("k1");
    generate(&WEIRDNET, Lang::Rust, "pub fn run(host: &Host) -> Result<View> { todo!() }", &dir).unwrap();
    let m = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(m.contains("name = \"wn_code_k1\""), "{m}");
    assert!(m.contains("weirdnet-plugin = { path = "), "{m}");
    assert!(m.ends_with("[workspace]\n"));
    assert_eq!(
        std::fs::read_to_string(dir.join("src/lib.rs")).unwrap(),
        "#![allow(unused, clippy::all)]\nmod user;\nweirdnet_plugin::plugin!(user::run, \"rust\");\n"
    );
    write_sdk(&WEIRDNET, &env.sdk_dir(&WEIRDNET)).unwrap();
    assert!(env.sdk_dir(&WEIRDNET).join("src/abi.rs").exists());
    let e = generate(&WEIRDNET, Lang::Rust, "nothing", &dir).unwrap_err().to_string();
    assert_eq!(e, "invalid rust: define `pub fn run(host: &Host) -> Result<View>`");
    let e = check(&WEIRDNET, &env, Lang::Rust, "nothing").unwrap_err().to_string();
    assert!(e.contains("use weirdnet_plugin::prelude::*;"), "{e}");
    assert!(generate(&WEIRDNET, Lang::Plugin, "/x.so", &dir).is_err());
}

#[cfg(feature = "math")]
#[test]
fn math_is_generated_against_the_profile_sdk() {
    let rs = math::generate(&WEIRDNET, "fast = sma(close, 10)").unwrap();
    assert!(rs.starts_with("// Generated by Weirdnet from a math code space. Do not edit.\n"));
    assert!(
        rs.contains("use weirdnet_plugin::prelude::*;")
            && rs.contains("weirdnet_plugin::columns::finish")
            && rs.contains("weirdnet_plugin::columns::Output {")
    );
    assert!(!rs.contains("muninn"));
    let other = Profile { app: "Other", sdk_crate: "other_sdk", math_help: " (see the manual)", ..WEIRDNET };
    assert!(math::generate(&other, "fast = sma(close, 10)").unwrap().contains("use other_sdk::series as s;"));
    let e = check(&other, &Env::new("/nonexistent", "/nonexistent"), Lang::Math, "x = nope(close)").unwrap_err().to_string();
    assert_eq!(e, "invalid math: line 1: unknown function `nope` (see the manual)");
    assert_eq!(math::parse("x = nope(close)").unwrap_err().to_string(), "invalid math: line 1: unknown function `nope`");
}

#[cfg(not(feature = "python"))]
#[test]
fn languages_left_out_say_which_feature() {
    let e = check(&WEIRDNET, &Env::new("/nonexistent", "/nonexistent"), Lang::Python, "def f(x: float) -> float:\n  return x").unwrap_err();
    assert_eq!(e.to_string(), "invalid code: python code spaces need this crate's `python` feature");
}

#[test]
fn a_missing_cargo_says_what_to_do() {
    let p = Profile { cargo_env: "WEIRDNET_CODE_SPACES_TEST_NO_CARGO", ..WEIRDNET };
    // SAFETY: only this test reads or writes this variable
    unsafe { std::env::set_var("WEIRDNET_CODE_SPACES_TEST_NO_CARGO", "/nonexistent/cargo") };
    let e = cargo(&p).unwrap_err().to_string();
    assert_eq!(
        e,
        "missing Rust toolchain: code spaces in math, polars, rust and python compile to plugins with cargo (/nonexistent/cargo not found). Install Rust from https://rustup.rs, or set WEIRDNET_CARGO to cargo; prebuilt plugins run without it."
    );
}

#[cfg(feature = "native-plugins")]
#[test]
fn libraries_that_are_not_plugins_are_refused() {
    let d = tempfile::tempdir().unwrap();
    assert!(load(&WEIRDNET, &d.path().join("missing.so")).unwrap_err().to_string().starts_with("invalid plugin: "));
    let f = d.path().join("junk.so");
    std::fs::write(&f, b"not a library").unwrap();
    assert!(load(&WEIRDNET, &f).is_err());
}
