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
    // A failed first-run init stops with a message and a pause; it never falls through to the TUI.
    let init = launch.find("keys init").unwrap();
    let after = &launch[init..];
    assert!(after.trim_start_matches(|c| c != '\n').trim_start().starts_with("if errorlevel 1 goto initfailed"), "check init right after it runs");
    let failed = &launch[launch.find(":initfailed").expect("an :initfailed label")..];
    assert!(failed.contains("pause") && failed.contains("exit /b 1") && !failed.contains(" tui"));
    assert!(after[..after.find(":tui").unwrap()].contains("pause"), "pause after init so the words can be written down");
}

#[test]
fn the_macos_app_is_universal_signed_and_versioned() {
    let sh = read("scripts/macos-app.sh");
    for want in [
        "lipo -create",
        "scripts/icons.sh icns",
        ".VolumeIcon.icns",
        "CFBundleShortVersionString",
        "codesign --force --deep --sign -",
        "hdiutil create",
        "keys init",
        " tui",
    ] {
        assert!(sh.contains(want), "scripts/macos-app.sh lacks {want}");
    }
}

#[test]
fn every_installer_icon_comes_from_the_one_source_image() {
    let svg = read("assets/icons/muninn.svg");
    assert!(svg.contains("<svg") && svg.contains("viewBox=\"0 0 512 512\""), "assets/icons/muninn.svg must be the square raven tile");
    assert!(svg.contains("var(--muninn-bg, #1a1b26)"), "the tile keeps its Tokyo Night fallbacks");
    assert!(read("scripts/windows-setup.sh").contains("scripts/icons.sh ico"));
    assert!(read("scripts/macos-app.sh").contains("scripts/icons.sh icns"));
    for gone in ["assets/icons/muninn.ico", "assets/icons/muninn.png"] {
        assert!(!Path::new(env!("CARGO_MANIFEST_DIR")).join(gone).exists(), "{gone} is generated, not committed");
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

#[test]
fn every_packaging_template_pins_the_workspace_version() {
    // One place: [workspace.package] version; scripts/release.sh moves every pin below with it.
    let ver = env!("CARGO_PKG_VERSION");
    let cargo: toml::Value = toml::from_str(&read("Cargo.toml")).unwrap();
    assert_eq!(cargo["workspace"]["package"]["version"].as_str(), Some(ver));
    for p in ["packaging/arch/PKGBUILD", "packaging/arch/muninn-bin/PKGBUILD"] {
        assert!(read(p).lines().any(|l| l == format!("pkgver={ver}")), "{p} should have pkgver={ver}");
    }
    assert!(read("site/lib/app.dart").contains(&format!("const version = '{ver}';")));
    let winget: Vec<_> = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/winget/manifests/w/WeirdBrains/Muninn"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(winget, [ver], "one winget template directory, named for the version");
    let release = read("scripts/release.sh");
    for p in [
        "packaging/arch/PKGBUILD",
        "packaging/arch/muninn-bin/PKGBUILD",
        "packaging/homebrew/Formula/muninn.rb",
        "packaging/homebrew/Casks/muninn.rb",
        "site/lib/app.dart",
        "docs/install.md",
    ] {
        assert!(release.contains(p), "scripts/release.sh does not bump {p}");
    }
}

#[test]
fn release_sh_is_safe_in_a_shared_checkout() {
    let sh = read("scripts/release.sh");
    for bad in ["git add -A\n", "git add .", "commit -a", "commit -qa", "git stash", "git reset", "git clean", "push"] {
        assert!(!sh.lines().filter(|l| !l.trim_start().starts_with('#')).any(|l| l.contains(bad.trim_end())), "release.sh must not run `{bad}`");
    }
    assert!(sh.contains("cargo test -q -p muninn-core --lib man"), "the man staleness test is in muninn-core");
    assert!(sh.contains("CARGO_BUILD_JOBS"));
    for s in ["scripts/dist.sh", "scripts/windows-setup.sh", "scripts/macos-app.sh"] {
        assert!(read(s).contains("scripts/version.sh"), "{s} should read the version with scripts/version.sh");
    }
}

#[test]
fn fill_sums_skips_missing_dmg_and_setup_exes_but_not_tarballs() {
    let python = std::process::Command::new("python3").arg("--version").output();
    if !python.is_ok_and(|o| o.status.success()) {
        eprintln!("skipped: no python3");
        return;
    }
    let ver = "9.9.9";
    let dist = tempfile::tempdir().unwrap();
    let sums = |names: &[&str]| {
        let s: String = names.iter().enumerate().map(|(i, n)| format!("{:064x}  {n}\n", i + 1)).collect();
        std::fs::write(dist.path().join("SHA256SUMS"), s).unwrap();
    };
    let run = || {
        std::process::Command::new("python3")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/fill-sums.py"))
            .args([ver, dist.path().to_str().unwrap()])
            .output()
            .unwrap()
    };
    let tarballs = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "x86_64-apple-darwin", "aarch64-apple-darwin"]
        .map(|t| format!("muninn-{ver}-{t}.tar.gz"));
    let mut names: Vec<&str> = tarballs.iter().map(String::as_str).collect();
    // only the tarballs: the cask and winget manifests are skipped with warnings
    sums(&names);
    let o = run();
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "{err}");
    assert!(err.contains("macos.dmg is not in SHA256SUMS; skipping the Homebrew cask"), "{err}");
    assert!(err.contains("windows-setup.exe is not in SHA256SUMS; skipping the winget manifests"), "{err}");
    let out = dist.path().join("packaging");
    assert!(out.join("arch/PKGBUILD").exists() && out.join("homebrew/Formula/muninn.rb").exists());
    assert!(!out.join("homebrew/Casks/muninn.rb").exists());
    assert!(!out.join(format!("winget/manifests/w/WeirdBrains/Muninn/{ver}")).exists());
    // everything there: every file, with the setup sums in the installer manifest
    let extra =
        [format!("muninn-{ver}-macos.dmg"), format!("muninn-{ver}-x86_64-windows-setup.exe"), format!("muninn-{ver}-aarch64-windows-setup.exe")];
    names.extend(extra.iter().map(String::as_str));
    sums(&names);
    let o = run();
    assert!(o.status.success() && !String::from_utf8_lossy(&o.stderr).contains("warning"), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(out.join("homebrew/Casks/muninn.rb").exists());
    let inst = std::fs::read_to_string(out.join(format!("winget/manifests/w/WeirdBrains/Muninn/{ver}/WeirdBrains.Muninn.installer.yaml"))).unwrap();
    assert!(inst.contains(&format!("{:064X}", 6)) && inst.contains(&format!("{:064X}", 7)), "{inst}");
    // a missing tarball still fails, naming it
    sums(&names[1..]);
    let o = run();
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("x86_64-unknown-linux-gnu.tar.gz is not in"), "{o:?}");
}
