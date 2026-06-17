use std::collections::HashSet;

use clap::Parser;

use crate::gac::{parse_hex, Era};
use crate::ranking::Filters;

#[derive(Parser, Debug)]
#[command(name = "culture-and-arts", version, about)]
pub struct Cli {
    #[arg(long, default_value_t = 400)]
    pub limit: usize,

    #[arg(long = "seed", value_name = "URL")]
    pub seeds: Vec<String>,

    #[arg(long = "era", value_name = "ERA")]
    pub eras: Vec<String>,

    #[arg(long = "min-mp", value_name = "MEGAPIXELS")]
    pub min_megapixels: Option<f32>,

    #[arg(long = "color", value_name = "HEX")]
    pub color: Option<String>,

    #[arg(long = "color-threshold", default_value_t = 90.0)]
    pub color_threshold: f64,

    #[arg(long)]
    pub probe: bool,

    #[arg(long = "fetch", value_name = "URL")]
    pub fetch: Option<String>,

    #[arg(long = "any-orientation")]
    pub any_orientation: bool,

    #[arg(long = "all-types")]
    pub all_types: bool,

    #[arg(long = "list")]
    pub list: bool,
}

fn era_from_str(s: &str) -> Option<Era> {
    match s.to_lowercase().as_str() {
        "ancient" => Some(Era::Ancient),
        "medieval" => Some(Era::Medieval),
        "renaissance" => Some(Era::Renaissance),
        "baroque" => Some(Era::Baroque),
        "romantic" => Some(Era::Romantic),
        "modern" => Some(Era::Modern),
        "contemporary" => Some(Era::Contemporary),
        "unknown" => Some(Era::Unknown),
        _ => None,
    }
}

impl Cli {
    pub fn filters(&self) -> Filters {
        let eras: HashSet<Era> = self.eras.iter().filter_map(|s| era_from_str(s)).collect();
        Filters {
            eras: if eras.is_empty() { None } else { Some(eras) },
            min_megapixels: self.min_megapixels,
            target_color: self.color.as_deref().and_then(parse_hex),
            color_threshold: self.color_threshold,
        }
    }

    pub fn seed_urls(&self) -> Vec<String> {
        let mut seeds: Vec<String> =
            crate::gac::SEED_ASSETS.iter().map(|s| s.to_string()).collect();
        seeds.extend(self.seeds.iter().cloned());
        seeds
    }
}
