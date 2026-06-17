use image::{imageops::FilterType, RgbImage};

pub fn dominant_colors(img: &RgbImage, k: usize) -> Vec<[u8; 3]> {
    let small = image::imageops::resize(img, 64, 64, FilterType::Triangle);

    let mut bins: std::collections::HashMap<u16, (u64, [u64; 3])> = std::collections::HashMap::new();
    for p in small.pixels() {
        let [r, g, b] = p.0;
        let key = (((r >> 5) as u16) << 6) | (((g >> 5) as u16) << 3) | ((b >> 5) as u16);
        let entry = bins.entry(key).or_insert((0, [0, 0, 0]));
        entry.0 += 1;
        entry.1[0] += r as u64;
        entry.1[1] += g as u64;
        entry.1[2] += b as u64;
    }

    let mut ranked: Vec<(u64, [u8; 3])> = bins
        .into_values()
        .map(|(count, sum)| {
            let avg = [
                (sum[0] / count) as u8,
                (sum[1] / count) as u8,
                (sum[2] / count) as u8,
            ];
            (count, avg)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    ranked.into_iter().take(k).map(|(_, c)| c).collect()
}

fn color_distance(a: [u8; 3], b: [u8; 3]) -> f64 {
    let dr = a[0] as f64 - b[0] as f64;
    let dg = a[1] as f64 - b[1] as f64;
    let db = a[2] as f64 - b[2] as f64;
    (0.30 * dr * dr + 0.59 * dg * dg + 0.11 * db * db).sqrt()
}

pub fn palette_matches(palette: &[[u8; 3]], target: [u8; 3], threshold: f64) -> bool {
    palette.iter().any(|&c| color_distance(c, target) <= threshold)
}

pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}
