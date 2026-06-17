use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use image::RgbImage;
use reqwest::Client;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Semaphore;

use super::metadata::{extract_asset_links, parse_asset_page, Asset};
use super::palette::dominant_colors;
use crate::download::TilePyramid;

const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";

const CRAWL_CONCURRENCY: usize = 6;
const MAX_INFLIGHT_REQUESTS: usize = 6;
const MAX_EXAMINED: usize = 12000;

pub const SEED_ASSETS: [&str; 6] = [
    "https://artsandculture.google.com/asset/wildflower-painting-of-red-grevillea/wwEzEHEBAqxv4w",
    "https://artsandculture.google.com/asset/anemonenfisch/jgEIuPigE5ONfw",
    "https://artsandculture.google.com/asset/belinda-clark-australian-captain/-gFwVH3C-hNxbw",
    "https://artsandculture.google.com/asset/books-and-scholar-s-utensils-jang-han-jong/-AGQH0sln3yCbA",
    "https://artsandculture.google.com/asset/borderlands/OQEi6WfImro-hw",
    "https://artsandculture.google.com/asset/an-old-picture-of-one-of-the-first-locomotives-in-india-journeying-across-the-thane-creek/6gHlET5gJZx11A",
];

#[derive(Debug, Clone)]
pub enum Discovered {
    Found { asset: Box<Asset>, total: usize },
    Done(usize),
    Throttled,
}

pub struct GacClient {
    http: Client,
    sem: Arc<Semaphore>,
}

impl GacClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .gzip(true)
            .build()
            .context("building HTTP client")?;
        Ok(Self {
            http,
            sem: Arc::new(Semaphore::new(MAX_INFLIGHT_REQUESTS)),
        })
    }

    pub fn http(&self) -> &Client {
        &self.http
    }

    async fn get_text(&self, url: &str) -> Result<String> {
        let _permit = self.sem.acquire().await.expect("semaphore open");
        Ok(self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }

    pub async fn fetch_asset(&self, url: &str) -> Option<Asset> {
        self.fetch_asset_and_links(url).await.0
    }

    async fn fetch_candidate(&self, url: &str) -> (Option<Asset>, Vec<String>) {
        let (asset, links) = self.fetch_asset_and_links(url).await;
        let asset = match asset {
            Some(mut a) => {
                a.max_size = self.fetch_max_size(&a).await;
                Some(a)
            }
            None => None,
        };
        (asset, links)
    }

    async fn fetch_asset_and_links(&self, url: &str) -> (Option<Asset>, Vec<String>) {
        match self.get_text(url).await {
            Ok(html) => (parse_asset_page(url, &html), extract_asset_links(&html)),
            Err(e) => {
                log::warn!("skip {url}: {e}");
                (None, Vec::new())
            }
        }
    }

    pub async fn fetch_thumbnail(&self, asset: &Asset, longest_edge: u32) -> Result<RgbImage> {
        let _permit = self.sem.acquire().await.expect("semaphore open");
        let bytes = self
            .http
            .get(asset.thumb_url(longest_edge))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        Ok(image::load_from_memory(&bytes)
            .context("decoding thumbnail")?
            .to_rgb8())
    }

    pub async fn fetch_max_size(&self, asset: &Asset) -> Option<(u32, u32)> {
        let xml = self.get_text(&asset.page.tile_info_url()).await.ok()?;
        TilePyramid::from_xml(&xml).ok().map(|p| p.max_size())
    }

    pub async fn enrich(&self, asset: &mut Asset) -> Result<()> {
        if asset.max_size.is_none() {
            asset.max_size = self.fetch_max_size(asset).await;
        }
        if asset.palette.is_empty() {
            if let Ok(thumb) = self.fetch_thumbnail(asset, 200).await {
                asset.palette = dominant_colors(&thumb, 5);
            }
        }
        Ok(())
    }
}

fn orientation_ok(want_portrait: Option<bool>, size: Option<(u32, u32)>) -> bool {
    match want_portrait {
        None => true,
        Some(want) => match size {
            Some((w, h)) => (h > w) == want,
            None => false,
        },
    }
}

pub async fn discover(
    client: &GacClient,
    seeds: Vec<String>,
    limit: usize,
    want_portrait: Option<bool>,
    art_only: bool,
    progress: Option<UnboundedSender<Discovered>>,
) -> Vec<Asset> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    for s in seeds {
        if visited.insert(s.clone()) {
            queue.push_back(s);
        }
    }

    let mut results: Vec<Asset> = Vec::new();
    let mut examined = 0usize;
    let mut pages_fetched = 0usize;
    let mut pages_productive = 0usize;

    while !queue.is_empty() && results.len() < limit && examined < MAX_EXAMINED {
        let mut batch = Vec::new();
        while batch.len() < CRAWL_CONCURRENCY {
            match queue.pop_front() {
                Some(u) => batch.push(u),
                None => break,
            }
        }

        let mut tasks = FuturesUnordered::new();
        for url in batch {
            tasks.push(async move { client.fetch_candidate(&url).await });
        }

        while let Some((asset, links)) = tasks.next().await {
            pages_fetched += 1;
            if asset.is_some() || !links.is_empty() {
                pages_productive += 1;
            }
            for link in links {
                if visited.insert(link.clone()) {
                    queue.push_back(link);
                }
            }
            let Some(asset) = asset else { continue };
            examined += 1;
            if !orientation_ok(want_portrait, asset.max_size) {
                continue;
            }
            if art_only && !asset.looks_like_visual_art() {
                continue;
            }
            results.push(asset.clone());
            if let Some(tx) = &progress {
                let _ = tx.send(Discovered::Found {
                    asset: Box::new(asset),
                    total: results.len(),
                });
            }
            if results.len() >= limit {
                break;
            }
        }

        if results.is_empty() && pages_fetched >= 8 && pages_productive == 0 {
            if let Some(tx) = &progress {
                let _ = tx.send(Discovered::Throttled);
            }
            return results;
        }
    }

    if let Some(tx) = &progress {
        let _ = tx.send(Discovered::Done(results.len()));
    }
    results
}
