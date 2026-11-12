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
    let app_cask = read("packaging/homebrew/Casks/muninn-app.rb");
    assert!(
        app_cask.contains("muninn-app-#{version}-universal-apple-darwin.dmg")
            && app_cask.contains("app \"Muninn.app\"")
            && app_cask.contains("cask \"muninn-app\"")
    );
    let formula = read("packaging/homebrew/Formula/muninn.rb");
    let ver = env!("CARGO_PKG_VERSION");
    for rb in [&cask, &app_cask, &formula] {
        assert!(rb.contains(&format!("version \"{ver}\"")), "homebrew version should be {ver}");
    }
}

#[test]
fn the_winget_manifests_use_the_inno_setup_installer() {
    let ver = env!("CARGO_PKG_VERSION");
    let dir = format!("packaging/winget/manifests/w/WeirdBrains/muninn/tui/{ver}");
    let inst = read(&format!("{dir}/WeirdBrains.muninn.tui.installer.yaml"));
    for want in ["InstallerType: inno", "Scope: user", "PackageIdentifier: WeirdBrains.muninn.tui"] {
        assert!(inst.contains(want), "winget installer lacks {want}");
    }
    for arch in ["x86_64", "aarch64"] {
        assert!(inst.contains(&format!("v{ver}/muninn-{ver}-{arch}-windows-setup.exe")));
    }
    assert!(read(&format!("{dir}/WeirdBrains.muninn.tui.yaml")).contains(&format!("PackageVersion: {ver}")));
}

#[test]
fn every_packaging_template_pins_the_workspace_version() {
    // One place: [workspace.package] version; release.toml (the shared release script) moves every pin below with it.
    let ver = env!("CARGO_PKG_VERSION");
    let cargo: toml::Value = toml::from_str(&read("Cargo.toml")).unwrap();
    assert_eq!(cargo["workspace"]["package"]["version"].as_str(), Some(ver));
    for p in ["packaging/arch/PKGBUILD", "packaging/arch/muninn-bin/PKGBUILD"] {
        assert!(read(p).lines().any(|l| l == format!("pkgver={ver}")), "{p} should have pkgver={ver}");
    }
    assert!(read("site/lib/app.dart").contains(&format!("const version = '{ver}';")));
    let winget: Vec<_> = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/winget/manifests/w/WeirdBrains/muninn/tui"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(winget, [ver], "one winget template directory, named for the version");
    let release = read("release.toml");
    for p in [
        "packaging/arch/PKGBUILD",
        "packaging/arch/muninn-bin/PKGBUILD",
        "packaging/homebrew/Formula/muninn.rb",
        "packaging/homebrew/Casks/muninn.rb",
        "packaging/homebrew/Casks/muninn-app.rb",
        "site/lib/app.dart",
        "docs/install.md",
    ] {
        assert!(release.contains(&format!("\"{p}\"")), "release.toml does not bump {p}");
    }
}

#[test]
fn cross_windows_sh_packages_from_cargo_target_dir() {
    let sh = read("scripts/cross-windows.sh");
    assert!(sh.contains(r#"td="${CARGO_TARGET_DIR:-target}""#), "scripts/cross-windows.sh must honour CARGO_TARGET_DIR");
    assert!(sh.contains(r#"scripts/dist.sh "$t" "$td/$t/release/mnnn.exe""#), "scripts/cross-windows.sh must pass the built exe path to dist.sh");
    // OpenSSL for aarch64 also lives under $td, not a hard-coded ./target
    assert!(sh.contains(r#"ossl="$td/openssl-$t""#) && !sh.contains("$PWD/target/openssl"), "{sh}");
}

#[test]
fn release_config_runs_the_man_and_packaging_checks() {
    let cfg = read("release.toml");
    assert!(cfg.contains("cargo test -q --locked -p muninn-core --lib man"), "the man staleness test is in muninn-core");
    assert!(cfg.contains("cargo test -q --locked --test packaging"));
    assert!(cfg.contains("CARGO_BUILD_JOBS"));
    assert!(cfg.contains(r#"stage = ["man"]"#), "only the regenerated man pages are staged beyond the version files");
    assert!(read("scripts/weirdnet_release.py").contains("__version__ = \""), "the vendored shared release script is present");
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
    assert!(err.contains("universal-apple-darwin.dmg is not in SHA256SUMS; skipping the Homebrew muninn-app cask"), "{err}");
    assert!(err.contains("windows-setup.exe is not in SHA256SUMS; skipping the winget manifests"), "{err}");
    let out = dist.path().join("packaging");
    assert!(out.join("arch/PKGBUILD").exists() && out.join("homebrew/Formula/muninn.rb").exists());
    assert!(!out.join("homebrew/Casks/muninn.rb").exists());
    assert!(!out.join("homebrew/Casks/muninn-app.rb").exists());
    assert!(!out.join(format!("winget/manifests/w/WeirdBrains/muninn/tui/{ver}")).exists());
    // everything there: every file, with the setup sums in the installer manifest
    let extra = [
        format!("muninn-{ver}-macos.dmg"),
        format!("muninn-{ver}-x86_64-windows-setup.exe"),
        format!("muninn-{ver}-aarch64-windows-setup.exe"),
        format!("muninn-app-{ver}-universal-apple-darwin.dmg"),
    ];
    names.extend(extra.iter().map(String::as_str));
    sums(&names);
    let o = run();
    assert!(o.status.success() && !String::from_utf8_lossy(&o.stderr).contains("warning"), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(out.join("homebrew/Casks/muninn.rb").exists());
    assert!(out.join("homebrew/Casks/muninn-app.rb").exists());
    let inst =
        std::fs::read_to_string(out.join(format!("winget/manifests/w/WeirdBrains/muninn/tui/{ver}/WeirdBrains.muninn.tui.installer.yaml"))).unwrap();
    assert!(inst.contains(&format!("{:064X}", 6)) && inst.contains(&format!("{:064X}", 7)), "{inst}");
    // a missing tarball still fails, naming it
    sums(&names[1..]);
    let o = run();
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("x86_64-unknown-linux-gnu.tar.gz is not in"), "{o:?}");
}

#[test]
fn muninn_bin_pkgbuild_source_urls_are_host_independent() {
    // $CARCH in source_x86_64/source_aarch64 expands to the host in `makepkg --printsrcinfo`,
    // so a TAUR/.SRCINFO upload would point both arches at one tarball.
    let pb = read("packaging/arch/muninn-bin/PKGBUILD");
    assert!(pb.contains(r#"source_x86_64=("$url/releases/download/v$pkgver/muninn-$pkgver-x86_64-unknown-linux-gnu.tar.gz")"#), "{pb}");
    assert!(pb.contains(r#"source_aarch64=("$url/releases/download/v$pkgver/muninn-$pkgver-aarch64-unknown-linux-gnu.tar.gz")"#), "{pb}");
    assert!(!pb.lines().any(|l| l.starts_with("source_") && l.contains("$CARCH")), "source_* arrays must not use $CARCH");
    let sh = read("scripts/publish-taur.sh");
    for needle in
        ["taurctl pkg upload", "--org \"$org\"", "org=WeirdBrains", "muninn-bin", "--dry-run", "makepkg --printsrcinfo", "SHA256SUMS", "isDraft"]
    {
        assert!(sh.contains(needle), "scripts/publish-taur.sh should contain {needle}");
    }
    assert!(
        !sh.contains("git push") && !sh.contains("gh release create") && !sh.contains("git tag"),
        "publish-taur.sh must not push git or make releases"
    );
    assert!(read(".github/workflows/ci.yml").contains("packaging/arch/test-publish-taur.sh"));
}
