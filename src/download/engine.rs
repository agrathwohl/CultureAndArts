use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use image::RgbImage;
use reqwest::Client;
use tokio::sync::mpsc::UnboundedSender;

use super::decryption::decrypt;
use super::tile_info::{PageInfo, PyramidLevel, TilePyramid};
use super::url::compute_tile_url;

const TILE_CONCURRENCY: usize = 16;

#[derive(Debug, Clone)]
pub enum Progress {
    Manifest { tiles: usize, size: (u32, u32) },
    Tile { done: usize, total: usize },
    Saved { path: PathBuf, size: (u32, u32) },
    Failed(String),
}

fn send(tx: &Option<UnboundedSender<Progress>>, p: Progress) {
    if let Some(s) = tx {
        let _ = s.send(p);
    }
}

async fn fetch_pyramid(client: &Client, page: &PageInfo) -> Result<TilePyramid> {
    let xml = client
        .get(page.tile_info_url())
        .send()
        .await
        .context("requesting tile manifest")?
        .error_for_status()
        .context("tile manifest HTTP status")?
        .text()
        .await
        .context("reading tile manifest body")?;
    TilePyramid::from_xml(&xml).map_err(|e| anyhow!("parsing tile manifest: {e}"))
}

async fn fetch_tile(
    client: Client,
    base_url: String,
    token: String,
    level_z: usize,
    tx: u32,
    ty: u32,
) -> Result<(u32, u32, RgbImage)> {
    let url = compute_tile_url(&base_url, &token, tx, ty, level_z);
    let bytes = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("requesting tile {tx},{ty}"))?
        .error_for_status()
        .with_context(|| format!("tile {tx},{ty} HTTP status"))?
        .bytes()
        .await
        .with_context(|| format!("reading tile {tx},{ty} body"))?
        .to_vec();

    let decrypted = decrypt(bytes).with_context(|| format!("decrypting tile {tx},{ty}"))?;
    let img = image::load_from_memory(&decrypted)
        .with_context(|| format!("decoding tile {tx},{ty}"))?
        .to_rgb8();
    Ok((tx, ty, img))
}

pub async fn download_max_res(
    client: &Client,
    page: &PageInfo,
    out_path: &Path,
    progress: Option<UnboundedSender<Progress>>,
) -> Result<(PathBuf, (u32, u32))> {
    let pyramid = fetch_pyramid(client, page).await?;
    let level: PyramidLevel = pyramid.max_level().clone();
    let (full_w, full_h) = (level.width, level.height);
    let total = (level.num_tiles_x * level.num_tiles_y) as usize;
    send(&progress, Progress::Manifest { tiles: total, size: (full_w, full_h) });

    let mut canvas = RgbImage::new(full_w, full_h);

    let mut tasks = FuturesUnordered::new();
    let mut coords = Vec::with_capacity(total);
    for ty in 0..level.num_tiles_y {
        for tx in 0..level.num_tiles_x {
            coords.push((tx, ty));
        }
    }
    let mut coord_iter = coords.into_iter();

    for _ in 0..TILE_CONCURRENCY {
        if let Some((tx, ty)) = coord_iter.next() {
            tasks.push(fetch_tile(client.clone(), page.base_url.clone(), page.token.clone(), level.z, tx, ty));
        }
    }

    let mut done = 0usize;
    while let Some(result) = tasks.next().await {
        let (tx, ty, tile) = match result {
            Ok(v) => v,
            Err(e) => {
                send(&progress, Progress::Failed(e.to_string()));
                return Err(e);
            }
        };
        let px = tx * pyramid.tile_width;
        let py = ty * pyramid.tile_height;
        image::imageops::replace(&mut canvas, &tile, px as i64, py as i64);

        done += 1;
        send(&progress, Progress::Tile { done, total });

        if let Some((nx, ny)) = coord_iter.next() {
            tasks.push(fetch_tile(client.clone(), page.base_url.clone(), page.token.clone(), level.z, nx, ny));
        }
    }

    if let Some(parent) = out_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    canvas
        .save(out_path)
        .with_context(|| format!("saving image to {}", out_path.display()))?;

    send(&progress, Progress::Saved { path: out_path.to_path_buf(), size: (full_w, full_h) });
    Ok((out_path.to_path_buf(), (full_w, full_h)))
}
