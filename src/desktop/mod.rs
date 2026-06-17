use std::path::{Path, PathBuf};
use std::process::Command;

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct DesktopProfile {
    pub desktop: String,
    pub session_type: String,
    pub resolution: Option<(u32, u32)>,
    pub scale: Option<f32>,
    pub font: Option<String>,
    pub cursor_theme: Option<String>,
    pub bars: Vec<String>,
}

impl DesktopProfile {
    pub fn aspect_ratio(&self) -> Option<f32> {
        self.resolution.map(|(w, h)| w as f32 / h as f32)
    }

    pub fn is_landscape(&self) -> bool {
        self.aspect_ratio().map(|r| r >= 1.0).unwrap_or(true)
    }

    pub fn megapixels(&self) -> Option<f32> {
        self.resolution.map(|(w, h)| (w as f32 * h as f32) / 1_000_000.0)
    }
}

pub fn probe() -> DesktopProfile {
    let desktop = env_first(&["XDG_CURRENT_DESKTOP", "DESKTOP_SESSION"]).unwrap_or_else(|| "unknown".into());
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());

    let (resolution, scale) = detect_geometry();
    let font = detect_font();
    let cursor_theme = detect_cursor();
    let bars = detect_bars();

    DesktopProfile {
        desktop,
        session_type,
        resolution,
        scale,
        font,
        cursor_theme,
        bars,
    }
}

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

fn detect_geometry() -> (Option<(u32, u32)>, Option<f32>) {
    if let Some(json) = run("hyprctl", &["monitors", "-j"]) {
        if let Some(g) = parse_hyprland(&json) {
            return g;
        }
    }
    if let Some(json) = run("swaymsg", &["-t", "get_outputs", "-r"]) {
        if let Some(g) = parse_sway(&json) {
            return g;
        }
    }
    if let Some(text) = run("wlr-randr", &[]) {
        if let Some(res) = parse_wlr_randr(&text) {
            return (Some(res), None);
        }
    }
    if let Some(text) = run("xrandr", &["--current"]) {
        if let Some(res) = parse_xrandr(&text) {
            return (Some(res), None);
        }
    }
    (None, None)
}

fn parse_hyprland(json: &str) -> Option<(Option<(u32, u32)>, Option<f32>)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let monitors = v.as_array()?;
    let mon = monitors
        .iter()
        .find(|m| m.get("focused").and_then(|f| f.as_bool()).unwrap_or(false))
        .or_else(|| monitors.first())?;
    let w = mon.get("width")?.as_u64()? as u32;
    let h = mon.get("height")?.as_u64()? as u32;
    let scale = mon.get("scale").and_then(|s| s.as_f64()).map(|s| s as f32);
    Some((Some((w, h)), scale))
}

fn parse_sway(json: &str) -> Option<(Option<(u32, u32)>, Option<f32>)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let outputs = v.as_array()?;
    let out = outputs
        .iter()
        .find(|o| o.get("active").and_then(|a| a.as_bool()).unwrap_or(false))
        .or_else(|| outputs.first())?;
    let mode = out.get("current_mode")?;
    let w = mode.get("width")?.as_u64()? as u32;
    let h = mode.get("height")?.as_u64()? as u32;
    let scale = out.get("scale").and_then(|s| s.as_f64()).map(|s| s as f32);
    Some((Some((w, h)), scale))
}

static WLR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d{3,5})x(\d{3,5}) px").unwrap());

fn parse_wlr_randr(text: &str) -> Option<(u32, u32)> {
    for line in text.lines() {
        if line.contains("current") {
            if let Some(c) = WLR_RE.captures(line) {
                return Some((c[1].parse().ok()?, c[2].parse().ok()?));
            }
        }
    }
    WLR_RE
        .captures(text)
        .and_then(|c| Some((c[1].parse().ok()?, c[2].parse().ok()?)))
}

static XRANDR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\bconnected\b.*?(\d{3,5})x(\d{3,5})\+\d+\+\d+").unwrap());

fn parse_xrandr(text: &str) -> Option<(u32, u32)> {
    let primary = text
        .lines()
        .find(|l| l.contains(" connected primary"))
        .or_else(|| text.lines().find(|l| l.contains(" connected")));
    let line = primary?;
    let c = XRANDR_RE.captures(line)?;
    Some((c[1].parse().ok()?, c[2].parse().ok()?))
}

static FCMATCH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#""([^"]+)""#).unwrap());
static KITTY_FONT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\s*font_family\s+(.+?)\s*$").unwrap());
static GHOSTTY_FONT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?m)^\s*font-family\s*=\s*"?([^"\n]+?)"?\s*$"#).unwrap());
static FOOT_FONT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*font\s*=\s*([^:\n]+)").unwrap());
static ALACRITTY_FONT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"family\s*=\s*"([^"]+)""#).unwrap());
static GTK_FONT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\s*gtk-font-name\s*=\s*(.+?)\s*$").unwrap());
static GTK_CURSOR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\s*gtk-cursor-theme-name\s*=\s*(.+?)\s*$").unwrap());

fn home_path(rel: &str) -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(rel))
}

fn file_capture(path: &Path, re: &Regex) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    re.captures(&text).map(|c| c[1].trim().to_string())
}

fn env_set(key: &str) -> bool {
    std::env::var_os(key).is_some()
}

fn terminal_font() -> Option<String> {
    if env_set("KITTY_WINDOW_ID") {
        if let Some(v) = home_path(".config/kitty/kitty.conf").and_then(|p| file_capture(&p, &KITTY_FONT_RE)) {
            return Some(v);
        }
    }
    if env_set("GHOSTTY_RESOURCES_DIR") {
        if let Some(v) = home_path(".config/ghostty/config").and_then(|p| file_capture(&p, &GHOSTTY_FONT_RE)) {
            return Some(v);
        }
    }
    if env_set("ALACRITTY_WINDOW_ID") || env_set("ALACRITTY_SOCKET") {
        if let Some(v) = home_path(".config/alacritty/alacritty.toml").and_then(|p| file_capture(&p, &ALACRITTY_FONT_RE)) {
            return Some(v);
        }
    }
    if std::env::var("TERM").unwrap_or_default().contains("foot") {
        if let Some(v) = home_path(".config/foot/foot.ini").and_then(|p| file_capture(&p, &FOOT_FONT_RE)) {
            return Some(v);
        }
    }
    None
}

fn strip_font_size(s: &str) -> String {
    match s.rsplit_once(' ') {
        Some((name, last)) if !last.is_empty() && last.chars().all(|c| c.is_ascii_digit()) => {
            name.trim().to_string()
        }
        _ => s.trim().to_string(),
    }
}

fn detect_font() -> Option<String> {
    if let Some(f) = terminal_font() {
        return Some(f);
    }
    if let Some(f) = home_path(".config/gtk-3.0/settings.ini").and_then(|p| file_capture(&p, &GTK_FONT_RE)) {
        return Some(strip_font_size(&f));
    }
    let out = run("fc-match", &["sans-serif"])?;
    FCMATCH_RE.captures(&out).map(|c| c[1].to_string())
}

fn dconf_value(key: &str) -> Option<String> {
    let out = run("dconf", &["read", key])?;
    let trimmed = out.trim().trim_matches('\'').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn compositor_env_cursor() -> Option<String> {
    let out = run("pgrep", &["-x", "Hyprland"])?;
    let pid = out.split_whitespace().next()?;
    let environ = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    for var in environ.split(|&b| b == 0) {
        let s = String::from_utf8_lossy(var);
        for key in ["HYPRCURSOR_THEME=", "XCURSOR_THEME="] {
            if let Some(v) = s.strip_prefix(key) {
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn detect_cursor() -> Option<String> {
    if let Some(theme) = env_first(&["HYPRCURSOR_THEME", "XCURSOR_THEME"]) {
        return Some(theme);
    }
    if let Some(theme) = compositor_env_cursor() {
        return Some(theme);
    }
    if let Some(v) = dconf_value("/org/gnome/desktop/interface/cursor-theme") {
        return Some(v);
    }
    for rel in [".config/gtk-3.0/settings.ini", ".config/gtk-4.0/settings.ini"] {
        if let Some(v) = home_path(rel).and_then(|p| file_capture(&p, &GTK_CURSOR_RE)) {
            return Some(v);
        }
    }
    if let Some(out) = run("gsettings", &["get", "org.gnome.desktop.interface", "cursor-theme"]) {
        let trimmed = out.trim().trim_matches('\'').to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    if let Some(theme) = hypr_cursor_from_config() {
        return Some(theme);
    }
    if std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase()
        .contains("hypr")
    {
        return Some("default (no theme configured)".into());
    }
    None
}

static HYPR_THEME_ENV_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?im)^\s*env\s*=\s*(?:HYPRCURSOR_THEME|XCURSOR_THEME)\s*,\s*(.+?)\s*$").unwrap()
});

fn collect_conf_files(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
    if depth == 0 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_conf_files(&path, out, depth - 1);
            } else if path.extension().map(|e| e == "conf").unwrap_or(false) {
                out.push(path);
            }
        }
    }
}

fn hypr_cursor_from_config() -> Option<String> {
    let dir = home_path(".config/hypr")?;
    let mut files = Vec::new();
    collect_conf_files(&dir, &mut files, 5);
    let mut dynamic = false;
    for path in files {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(c) = HYPR_THEME_ENV_RE.captures(&text) {
                return Some(c[1].trim().to_string());
            }
            if text.contains("dynamic-cursors") || text.contains("dynamic_cursors") {
                dynamic = true;
            }
        }
    }
    if dynamic {
        Some("hypr-dynamic-cursors (animated; no base theme set)".into())
    } else {
        None
    }
}

const KNOWN_BARS: [&str; 10] = [
    "quickshell",
    "waybar",
    "polybar",
    "plasmashell",
    "gnome-shell",
    "swaybar",
    "i3bar",
    "eww",
    "ironbar",
    "hyprpanel",
];

fn detect_bars() -> Vec<String> {
    let pattern = KNOWN_BARS.join("|");
    let mut found = Vec::new();
    if let Some(out) = run("pgrep", &["-l", "-f", pattern.as_str()]) {
        for bar in KNOWN_BARS {
            if out.contains(bar) {
                found.push(bar.to_string());
            }
        }
    }
    found
}
