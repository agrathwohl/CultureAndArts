use std::collections::HashSet;

use crate::desktop::DesktopProfile;
use crate::gac::{palette_matches, Asset, Era};

#[derive(Debug, Clone, Default)]
pub struct Filters {
    pub eras: Option<HashSet<Era>>,
    pub min_megapixels: Option<f32>,
    pub target_color: Option<[u8; 3]>,
    pub color_threshold: f64,
}

impl Filters {
    pub fn passes(&self, asset: &Asset) -> bool {
        if let Some(eras) = &self.eras {
            if !eras.contains(&asset.era()) {
                return false;
            }
        }
        if let Some(min_mp) = self.min_megapixels {
            match asset.max_size {
                Some((w, h)) if (w as f32 * h as f32) / 1_000_000.0 >= min_mp => {}
                _ => return false,
            }
        }
        if let Some(color) = self.target_color {
            if asset.palette.is_empty() || !palette_matches(&asset.palette, color, self.color_threshold) {
                return false;
            }
        }
        true
    }
}

pub fn score(asset: &Asset, profile: &DesktopProfile) -> f32 {
    let (screen_ratio, asset_ratio) = match (profile.aspect_ratio(), asset.max_size) {
        (Some(s), Some((w, h))) if h > 0 && w > 0 => (s, w as f32 / h as f32),
        _ => return 0.1,
    };

    let aspect_close = 1.0 / (1.0 + (screen_ratio.ln() - asset_ratio.ln()).abs());

    let orientation = if (screen_ratio >= 1.0) == (asset_ratio >= 1.0) {
        1.0
    } else {
        0.15
    };

    let resolution = match profile.megapixels() {
        Some(smp) if smp > 0.0 => {
            let amp = asset.max_size.map(|(w, h)| w as f32 * h as f32 / 1_000_000.0).unwrap_or(0.0);
            (amp / smp).min(1.0)
        }
        _ => 0.5,
    };

    (0.7 * aspect_close + 0.3 * resolution) * orientation
}

pub fn rank(mut assets: Vec<Asset>, profile: &DesktopProfile, filters: &Filters) -> Vec<Asset> {
    assets.retain(|a| filters.passes(a));
    assets.sort_by(|a, b| {
        score(b, profile)
            .partial_cmp(&score(a, profile))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    assets
}
