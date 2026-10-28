//! System light/dark detection: terminal background (OSC 11), then the OS preference, else dark.
use super::Variant;
use std::io::IsTerminal;
use std::process::Command;

pub fn system_variant() -> Variant {
    terminal_variant().or_else(os_variant).unwrap_or(Variant::Dark)
}

/// Query the terminal background (OSC 11) when attached to a terminal.
pub fn terminal_variant() -> Option<Variant> {
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return None;
    }
    let luma = terminal_light::luma().ok()?;
    Some(if luma < 0.5 { Variant::Dark } else { Variant::Light })
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(cmd).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).to_string())
}

/// OS-level preference.
pub fn os_variant() -> Option<Variant> {
    if cfg!(target_os = "macos") {
        // Prints "Dark" in dark mode; fails (key absent) in light mode.
        return Some(match run("defaults", &["read", "-g", "AppleInterfaceStyle"]) {
            Some(s) if s.trim() == "Dark" => Variant::Dark,
            _ => Variant::Light,
        });
    }
    if cfg!(windows) {
        let s = run(
            "reg",
            &["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "/v", "AppsUseLightTheme"],
        )?;
        return Some(if s.contains("0x0") { Variant::Dark } else { Variant::Light });
    }
    // freedesktop portal: org.freedesktop.appearance color-scheme (1 = dark, 2 = light, 0 = no preference)
    let out = run(
        "busctl",
        &[
            "--user", "call", "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings", "ReadOne", "ss", "org.freedesktop.appearance", "color-scheme",
        ],
    )
    .or_else(|| {
        run(
            "gdbus",
            &[
                "call", "--session", "--dest", "org.freedesktop.portal.Desktop", "--object-path",
                "/org/freedesktop/portal/desktop", "--method", "org.freedesktop.portal.Settings.ReadOne",
                "org.freedesktop.appearance", "color-scheme",
            ],
        )
    })?;
    parse_portal(&out)
}

/// Parse `v u 1` (busctl) or `(<uint32 1>,)` (gdbus).
pub fn parse_portal(s: &str) -> Option<Variant> {
    let n = s.chars().rev().find(|c| c.is_ascii_digit())?;
    match n {
        '1' => Some(Variant::Dark),
        '2' => Some(Variant::Light),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portal_parsing() {
        assert_eq!(parse_portal("v u 1\n"), Some(Variant::Dark));
        assert_eq!(parse_portal("(<<uint32 2>>,)\n"), Some(Variant::Light));
        assert_eq!(parse_portal("v u 0"), None);
    }
}
