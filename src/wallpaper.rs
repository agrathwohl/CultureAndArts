use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Result};

fn binary_exists(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
    })
}

fn process_running(name: &str) -> bool {
    Command::new("pgrep")
        .args(["-x", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| anyhow!("failed to run {cmd}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{cmd} exited unsuccessfully"))
    }
}

fn spawn_detached(cmd: &str, args: &[&str]) -> Result<()> {
    let (bin, full): (&str, Vec<&str>) = if binary_exists("setsid") {
        let mut v = vec![cmd];
        v.extend_from_slice(args);
        ("setsid", v)
    } else {
        (cmd, args.to_vec())
    };
    Command::new(bin)
        .args(&full)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("failed to spawn {cmd}: {e}"))?;
    Ok(())
}

pub fn set_auto(path: &Path) -> Result<String> {
    let p = path
        .to_str()
        .ok_or_else(|| anyhow!("wallpaper path is not valid UTF-8"))?;

    if binary_exists("waypaper") {
        run("waypaper", &["--wallpaper", p])?;
        return Ok("waypaper".into());
    }
    if binary_exists("swww") {
        if run("swww", &["query"]).is_err() {
            let _ = spawn_detached("swww-daemon", &[]);
            std::thread::sleep(std::time::Duration::from_millis(700));
        }
        run("swww", &["img", p, "--transition-type", "any"])?;
        return Ok("swww".into());
    }
    if process_running("hyprpaper") && binary_exists("hyprctl") {
        run("hyprctl", &["hyprpaper", "preload", p])?;
        run("hyprctl", &["hyprpaper", "wallpaper", &format!(",{p}")])?;
        return Ok("hyprpaper".into());
    }
    if binary_exists("swaybg") {
        let _ = run("pkill", &["-x", "swaybg"]);
        spawn_detached("swaybg", &["-o", "*", "-i", p, "-m", "fill"])?;
        return Ok("swaybg".into());
    }
    if binary_exists("wbg") {
        let _ = run("pkill", &["-x", "wbg"]);
        spawn_detached("wbg", &[p])?;
        return Ok("wbg".into());
    }
    if binary_exists("feh") {
        run("feh", &["--bg-fill", p])?;
        return Ok("feh".into());
    }
    if binary_exists("gsettings") {
        let uri = format!("file://{p}");
        run("gsettings", &["set", "org.gnome.desktop.background", "picture-uri", &uri])?;
        run("gsettings", &["set", "org.gnome.desktop.background", "picture-uri-dark", &uri])?;
        return Ok("gsettings".into());
    }
    Err(anyhow!(
        "no wallpaper setter found (install waypaper, swww, hyprpaper, swaybg, wbg, feh, or gsettings)"
    ))
}
