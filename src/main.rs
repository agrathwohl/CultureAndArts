mod cli;
mod config;
mod desktop;
mod download;
mod gac;
mod ranking;
mod tui;
mod wallpaper;

use anyhow::{Context, Result};
use clap::Parser;

use config::output_path_for;
use desktop::DesktopProfile;
use download::download_max_res;
use gac::GacClient;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Cli::parse();
    let profile = desktop::probe();

    if args.probe {
        print_profile(&profile);
        return Ok(());
    }

    let client = GacClient::new()?;

    if let Some(url) = args.fetch.as_deref() {
        return headless_fetch(&client, url).await;
    }

    let want_portrait = if args.any_orientation {
        None
    } else {
        profile.aspect_ratio().map(|r| r < 1.0)
    };

    let art_only = !args.all_types;

    if args.list {
        return headless_list(&client, args.seed_urls(), args.limit, want_portrait, art_only).await;
    }

    tui::run(profile, client, args.seed_urls(), args.limit, args.filters(), want_portrait, art_only).await
}

async fn headless_list(
    client: &GacClient,
    seeds: Vec<String>,
    limit: usize,
    want_portrait: Option<bool>,
    art_only: bool,
) -> Result<()> {
    let results = gac::discover(client, seeds, limit, want_portrait, art_only, None).await;
    for a in &results {
        let (w, h) = a.max_size.unwrap_or((0, 0));
        let orient = if h > w { "portrait" } else { "landscape" };
        let date = if a.date_text.is_empty() { "—" } else { &a.date_text };
        println!("{w:>6}×{h:<6} {orient:9} {date:<24} {} — {}", a.title, a.creator);
    }
    if results.is_empty() {
        println!(
            "\n0 works found — Google Arts & Culture is likely rate-limiting this IP. Wait several minutes and try again."
        );
    } else {
        println!("\n{} works found", results.len());
    }
    Ok(())
}

async fn headless_fetch(client: &GacClient, url: &str) -> Result<()> {
    let asset = client
        .fetch_asset(url)
        .await
        .context("could not parse that artwork page")?;
    let out = output_path_for(&asset);
    println!("Downloading max-resolution “{}” by {}…", asset.title, if asset.creator.is_empty() { "unknown" } else { &asset.creator });
    let (path, (w, h)) = download_max_res(client.http(), &asset.page, &out, None).await?;
    println!("Saved {w}×{h} → {}", path.display());
    Ok(())
}

fn print_profile(p: &DesktopProfile) {
    println!("CultureAndArts — detected desktop profile");
    println!(
        "  resolution : {}",
        p.resolution.map(|(w, h)| format!("{w}×{h}")).unwrap_or_else(|| "unknown".into())
    );
    println!("  scale      : {}", p.scale.map(|s| format!("{s:.2}")).unwrap_or_else(|| "unknown".into()));
    println!("  desktop    : {} ({})", p.desktop, p.session_type);
    println!("  font       : {}", p.font.clone().unwrap_or_else(|| "unknown".into()));
    println!("  cursor     : {}", p.cursor_theme.clone().unwrap_or_else(|| "unknown".into()));
    println!(
        "  bars       : {}",
        if p.bars.is_empty() { "none detected".into() } else { p.bars.join(", ") }
    );
    println!(
        "  aspect     : {}",
        p.aspect_ratio().map(|r| format!("{r:.3} ({})", if p.is_landscape() { "landscape" } else { "portrait" })).unwrap_or_else(|| "unknown".into())
    );
}
