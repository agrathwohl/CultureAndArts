use std::path::PathBuf;

use crate::gac::Asset;

fn pictures_dir() -> PathBuf {
    dirs::picture_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Pictures")
    })
}

fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '-' })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string()
}

pub fn output_path_for(asset: &Asset) -> PathBuf {
    let mut stem = String::new();
    if !asset.creator.is_empty() {
        stem.push_str(&sanitize(&asset.creator));
        stem.push_str(" - ");
    }
    stem.push_str(&sanitize(&asset.title));
    if stem.len() > 120 {
        stem.truncate(120);
    }
    let filename = format!("{stem} ({}).png", asset.id);
    pictures_dir().join(filename)
}
