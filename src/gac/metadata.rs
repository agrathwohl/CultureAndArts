use once_cell::sync::Lazy;
use regex::Regex;

use crate::download::PageInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Era {
    Ancient,
    Medieval,
    Renaissance,
    Baroque,
    Romantic,
    Modern,
    Contemporary,
    Unknown,
}

impl Era {
    pub fn from_year(year: Option<i32>) -> Era {
        match year {
            None => Era::Unknown,
            Some(y) if y < 500 => Era::Ancient,
            Some(y) if y < 1400 => Era::Medieval,
            Some(y) if y < 1600 => Era::Renaissance,
            Some(y) if y < 1750 => Era::Baroque,
            Some(y) if y < 1850 => Era::Romantic,
            Some(y) if y < 1945 => Era::Modern,
            Some(_) => Era::Contemporary,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Era::Ancient => "Ancient",
            Era::Medieval => "Medieval",
            Era::Renaissance => "Renaissance",
            Era::Baroque => "Baroque",
            Era::Romantic => "Romantic",
            Era::Modern => "Modern",
            Era::Contemporary => "Contemporary",
            Era::Unknown => "Unknown",
        }
    }

    pub const ALL: [Era; 8] = [
        Era::Ancient,
        Era::Medieval,
        Era::Renaissance,
        Era::Baroque,
        Era::Romantic,
        Era::Modern,
        Era::Contemporary,
        Era::Unknown,
    ];
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub id: String,
    pub url: String,
    pub title: String,
    pub creator: String,
    pub date_text: String,
    pub year: Option<i32>,
    pub medium: String,
    pub thumb_base: String,
    pub page: PageInfo,
    pub max_size: Option<(u32, u32)>,
    pub palette: Vec<[u8; 3]>,
}

const OBJECT_KEYWORDS: &[&str] = &[
    "vase", "vessel", "bowl", "cup", "jar", "jug", "urn", "ewer", "plate", "dish", "platter",
    "teapot", "bottle", "flask", "goblet", "chalice", "tankard", "pitcher", "saucer",
    "dress", "gown", "robe", "costume", "coat", "jacket", "trousers", "skirt", "blouse", "shoe",
    "boot", "sandal", "hat", "cap", "glove", "kimono", "uniform", "cloak", "corset", "waistcoat",
    "stool", "chair", "table", "desk", "cabinet", "chest", "bench", "sofa", "wardrobe", "dresser",
    "commode", "armchair", "throne",
    "sculpture", "statue", "statuette", "figurine", "bust", "carving", "torso",
    "coin", "medal", "medallion", "banknote",
    "ring", "necklace", "bracelet", "brooch", "pendant", "earring", "jewel", "tiara", "diadem",
    "sword", "dagger", "rifle", "pistol", "cannon", "armour", "armor", "helmet", "shield", "musket",
    "clock", "candlestick", "candelabra", "censer",
    "fragment", "sherd", "shard", "amulet", "scarab",
    "ceramic", "porcelain", "pottery", "earthenware", "stoneware", "terracotta", "majolica",
    "faience", "furniture", "cutlery", "spoon",
    "violin", "guitar", "lute", "cello", "mandolin",
];

const ART_KEYWORDS: &[&str] = &[
    "painting", "landscape", "still life", "drawing", "fresco", "watercolo", "tempera", "gouache",
    "etching", "engraving", "lithograph", "woodcut", "woodblock", "mezzotint", "aquatint", "pastel",
    "charcoal", "photograph", "oil on", "on canvas", "on paper", "on panel", "miniature",
];

impl Asset {
    pub fn looks_like_visual_art(&self) -> bool {
        let hay = format!("{} {}", self.title, self.medium).to_lowercase();
        if ART_KEYWORDS.iter().any(|k| hay.contains(k)) {
            return true;
        }
        for raw in hay.split(|c: char| !c.is_alphanumeric()) {
            if raw.is_empty() {
                continue;
            }
            let singular = raw.strip_suffix('s').unwrap_or(raw);
            if OBJECT_KEYWORDS.contains(&raw) || OBJECT_KEYWORDS.contains(&singular) {
                return false;
            }
        }
        true
    }
}

impl Asset {
    pub fn era(&self) -> Era {
        Era::from_year(self.year)
    }

    pub fn thumb_url(&self, longest_edge: u32) -> String {
        format!("{}=s{}", self.thumb_base, longest_edge)
    }
}

static OG_TITLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"<meta property="og:title" content="([^"]*)""#).unwrap());
static OG_IMAGE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"<meta property="og:image" content="([^"]*)""#).unwrap());
static DATE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""(?:Date Created|Dated|Date)",\[\["([^"]+)""#).unwrap());
static CREATOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#""Creator",\[\["([^"]+)""#).unwrap());
static KIND_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""(?:Object Type|Type|Medium|Classification)",\[\["([^"]+)""#).unwrap());
static YEAR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(\d{3,4})"#).unwrap());
static CENTURY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)(\d{1,2})(?:st|nd|rd|th)\s+century"#).unwrap());

const GAC_SUFFIX: &str = " - Google Arts & Culture";

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn asset_id_from_url(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

fn parse_year(date_text: &str) -> Option<i32> {
    if let Some(c) = CENTURY_RE.captures(date_text) {
        if let Ok(century) = c[1].parse::<i32>() {
            return Some((century - 1) * 100 + 50);
        }
    }
    YEAR_RE
        .captures(date_text)
        .and_then(|c| c[1].parse::<i32>().ok())
        .filter(|y| *y >= 1 && *y <= 2100)
}

fn split_title_creator(og_title: &str) -> (String, String) {
    let decoded = decode_entities(og_title);
    let cleaned = decoded.trim_end_matches(GAC_SUFFIX).trim_end_matches(" -").trim();
    match cleaned.rsplit_once(" - ") {
        Some((title, creator)) => (title.trim().to_string(), creator.trim().to_string()),
        None => (cleaned.to_string(), String::new()),
    }
}

pub fn parse_asset_page(url: &str, html: &str) -> Option<Asset> {
    let page = PageInfo::from_page_html(html).ok()?;
    let thumb_base = OG_IMAGE.captures(html).map(|c| c[1].to_string())?;

    let og_title = OG_TITLE.captures(html).map(|c| c[1].to_string()).unwrap_or_default();
    let (title, creator_from_title) = split_title_creator(&og_title);
    let creator = if creator_from_title.is_empty() {
        CREATOR_RE
            .captures(html)
            .map(|c| decode_entities(&c[1]))
            .unwrap_or_default()
    } else {
        creator_from_title
    };

    let date_text = DATE_RE
        .captures(html)
        .map(|c| decode_entities(&c[1]))
        .unwrap_or_default();
    let year = parse_year(&date_text);

    let medium = KIND_RE
        .captures_iter(html)
        .map(|c| decode_entities(&c[1]))
        .collect::<Vec<_>>()
        .join(" ");

    Some(Asset {
        id: asset_id_from_url(url),
        url: url.to_string(),
        title: if title.is_empty() { "Untitled".to_string() } else { title },
        creator,
        date_text,
        year,
        medium,
        thumb_base,
        page,
        max_size: None,
        palette: Vec::new(),
    })
}

static ASSET_LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/asset/[a-z0-9\-]+/[A-Za-z0-9_\-]{6,}"#).unwrap());

pub fn extract_asset_links(html: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for m in ASSET_LINK_RE.find_iter(html) {
        seen.insert(format!("https://artsandculture.google.com{}", m.as_str()));
    }
    seen.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_title_and_creator() {
        let (t, c) = split_title_creator(
            "Wildflower painting of Red Grevillea - Ellis Rowan - Google Arts &amp; Culture",
        );
        assert_eq!(t, "Wildflower painting of Red Grevillea");
        assert_eq!(c, "Ellis Rowan");
    }

    #[test]
    fn parses_circa_year_into_modern_era() {
        assert_eq!(parse_year("Marian) Ellis Rowan, Australia, c. 1900."), Some(1900));
        assert_eq!(Era::from_year(Some(1900)), Era::Modern);
    }

    #[test]
    fn parses_century_phrase() {
        assert_eq!(parse_year("late 17th century"), Some(1650));
    }

    fn mk(title: &str, medium: &str) -> Asset {
        Asset {
            id: "id".into(),
            url: "url".into(),
            title: title.into(),
            creator: String::new(),
            date_text: String::new(),
            year: None,
            medium: medium.into(),
            thumb_base: String::new(),
            page: crate::download::PageInfo {
                base_url: String::new(),
                token: String::new(),
            },
            max_size: None,
            palette: Vec::new(),
        }
    }

    #[test]
    fn art_classifier_keeps_paintings_rejects_objects() {
        assert!(mk("Wildflower painting of Red Grevillea", "Watercolour").looks_like_visual_art());
        assert!(mk("Carnation, Lily, Lily, Rose", "Oil on canvas").looks_like_visual_art());
        assert!(mk("Vase with Flowers", "Oil on canvas").looks_like_visual_art());
        assert!(mk("Portrait of a Man", "").looks_like_visual_art());

        assert!(!mk("Dress", "Silk").looks_like_visual_art());
        assert!(!mk("L-legged stool", "Wood").looks_like_visual_art());
        assert!(!mk("Celadon Vase", "Porcelain").looks_like_visual_art());
        assert!(!mk("Portrait Bust of a Man", "Marble").looks_like_visual_art());
    }
}
