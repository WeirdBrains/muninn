//! Installer and package metadata: the deb's glibc floor, the Windows setup script and the macOS app script.
use std::path::Path;

fn read(p: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(p)).unwrap()
}

#[test]
fn the_deb_requires_glibc_2_39() {
    let cargo: toml::Value = toml::from_str(&read("Cargo.toml")).unwrap();
    let depends = cargo["package"]["metadata"]["deb"]["depends"].as_str().unwrap();
    assert!(depends.contains("libc6 (>= 2.39)"), "deb depends should require libc6 >= 2.39: {depends}");
}

#[test]
fn the_windows_setup_installs_per_user_and_manages_path() {
    let iss = read("packaging/windows/muninn.iss");
    for want in [
        "PrivilegesRequired=lowest",
        "DefaultDirName={userpf}\\Muninn",
        "ChangesEnvironment=yes",
        "SetupIconFile=",
        "RemovePath",
        "muninn-launch.cmd",
    ] {
        assert!(iss.contains(want), "muninn.iss lacks {want}");
    }
    let launch = read("packaging/windows/muninn-launch.cmd");
    assert!(launch.contains("\r\n"), "muninn-launch.cmd must use CRLF line endings");
    assert!(launch.contains("wt.exe") && launch.contains("keys init") && launch.contains(" tui"));
}

#[test]
fn the_macos_app_is_universal_signed_and_versioned() {
    let sh = read("scripts/macos-app.sh");
    for want in ["lipo -create", "iconutil", "CFBundleShortVersionString", "codesign --force --deep --sign -", "hdiutil create", "keys init", " tui"]
    {
        assert!(sh.contains(want), "scripts/macos-app.sh lacks {want}");
    }
}

#[test]
fn the_icons_exist() {
    for f in ["assets/icons/muninn.png", "assets/icons/muninn.ico"] {
        assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join(f).metadata().unwrap().len() > 1000, "{f} missing");
    }
}

#[test]
fn the_homebrew_tap_has_a_formula_and_a_cask() {
    let cask = read("packaging/homebrew/Casks/muninn.rb");
    assert!(cask.contains("muninn-#{version}-macos.dmg") && cask.contains("app \"Muninn.app\""));
    let formula = read("packaging/homebrew/Formula/muninn.rb");
    let ver = env!("CARGO_PKG_VERSION");
    for rb in [&cask, &formula] {
        assert!(rb.contains(&format!("version \"{ver}\"")), "homebrew version should be {ver}");
    }
}

#[test]
fn the_winget_manifests_use_the_inno_setup_installer() {
    let ver = env!("CARGO_PKG_VERSION");
    let dir = format!("packaging/winget/manifests/w/WeirdBrains/Muninn/{ver}");
    let inst = read(&format!("{dir}/WeirdBrains.Muninn.installer.yaml"));
    for want in ["InstallerType: inno", "Scope: user", "PackageIdentifier: WeirdBrains.Muninn"] {
        assert!(inst.contains(want), "winget installer lacks {want}");
    }
    for arch in ["x86_64", "aarch64"] {
        assert!(inst.contains(&format!("v{ver}/muninn-{ver}-{arch}-windows-setup.exe")));
    }
    assert!(read(&format!("{dir}/WeirdBrains.Muninn.yaml")).contains(&format!("PackageVersion: {ver}")));
}
