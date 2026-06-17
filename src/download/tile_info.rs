use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct PageInfo {
    pub base_url: String,
    pub token: String,
}

static PAGE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"]\r?\n?,"(//[a-zA-Z0-9./_\-]+)",(?:"([^"]+)"|null)"#).unwrap());

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("could not locate the tiled-image token in the page")]
    NoToken,
    #[error("tile manifest contained no pyramid levels")]
    NoLevels,
    #[error("tile manifest missing {0} attribute")]
    MissingAttr(&'static str),
}

impl PageInfo {
    pub fn from_page_html(html: &str) -> Result<Self, ParseError> {
        let caps = PAGE_RE.captures(html).ok_or(ParseError::NoToken)?;
        let base_url = format!("https:{}", &caps[1]);
        let token = caps.get(2).map_or(String::new(), |m| m.as_str().to_string());
        Ok(PageInfo { base_url, token })
    }

    pub fn tile_info_url(&self) -> String {
        format!("{}=g", self.base_url)
    }
}

#[derive(Debug, Clone)]
pub struct PyramidLevel {
    pub z: usize,
    pub num_tiles_x: u32,
    pub num_tiles_y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct TilePyramid {
    pub tile_width: u32,
    pub tile_height: u32,
    pub levels: Vec<PyramidLevel>,
}

static TW_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"tile_width="(\d+)""#).unwrap());
static TH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"tile_height="(\d+)""#).unwrap());
static PLEVEL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"<pyramid_level\b[^>]*>"#).unwrap());
static NTX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"num_tiles_x="(\d+)""#).unwrap());
static NTY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"num_tiles_y="(\d+)""#).unwrap());
static EPX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"empty_pels_x="(\d+)""#).unwrap());
static EPY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"empty_pels_y="(\d+)""#).unwrap());

fn cap_u32(re: &Regex, s: &str) -> Option<u32> {
    re.captures(s).and_then(|c| c[1].parse().ok())
}

impl TilePyramid {
    pub fn from_xml(xml: &str) -> Result<Self, ParseError> {
        let tile_width = cap_u32(&TW_RE, xml).ok_or(ParseError::MissingAttr("tile_width"))?;
        let tile_height = cap_u32(&TH_RE, xml).ok_or(ParseError::MissingAttr("tile_height"))?;

        let mut levels = Vec::new();
        for (z, m) in PLEVEL_RE.find_iter(xml).enumerate() {
            let tag = m.as_str();
            let num_tiles_x = cap_u32(&NTX_RE, tag).unwrap_or(0);
            let num_tiles_y = cap_u32(&NTY_RE, tag).unwrap_or(0);
            if num_tiles_x == 0 || num_tiles_y == 0 {
                continue;
            }
            let empty_pels_x = cap_u32(&EPX_RE, tag).unwrap_or(0);
            let empty_pels_y = cap_u32(&EPY_RE, tag).unwrap_or(0);
            levels.push(PyramidLevel {
                z,
                num_tiles_x,
                num_tiles_y,
                width: tile_width * num_tiles_x - empty_pels_x,
                height: tile_height * num_tiles_y - empty_pels_y,
            });
        }

        if levels.is_empty() {
            return Err(ParseError::NoLevels);
        }
        Ok(TilePyramid {
            tile_width,
            tile_height,
            levels,
        })
    }

    pub fn max_level(&self) -> &PyramidLevel {
        self.levels.last().expect("pyramid validated non-empty")
    }

    pub fn max_size(&self) -> (u32, u32) {
        let l = self.max_level();
        (l.width, l.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_NEW: &str = r#"<TileInfo tile_width="512" tile_height="512" full_pyramid_depth="3" timestamp="1781451536">
        <pyramid_level num_tiles_x="1" num_tiles_y="1" empty_pels_x="286" empty_pels_y="192"/>
        <pyramid_level num_tiles_x="2" num_tiles_y="1" empty_pels_x="345" empty_pels_y="2"/>
        <pyramid_level num_tiles_x="11" num_tiles_y="8" empty_pels_x="196" empty_pels_y="16"/>
    </TileInfo>"#;

    const SAMPLE_OLD: &str = r#"<TileInfo tile_width="512" tile_height="512">
        <pyramid_level num_tiles_x="1" num_tiles_y="1" inverse_scale="16" empty_pels_x="173" empty_pels_y="257"/>
        <pyramid_level num_tiles_x="11" num_tiles_y="8" inverse_scale="1" empty_pels_x="196" empty_pels_y="16"/>
    </TileInfo>"#;

    #[test]
    fn parses_new_format_without_inverse_scale() {
        let p = TilePyramid::from_xml(SAMPLE_NEW).unwrap();
        assert_eq!(p.tile_width, 512);
        assert_eq!(p.levels.len(), 3);
        assert_eq!(p.max_size(), (5436, 4080));
    }

    #[test]
    fn parses_old_format_with_inverse_scale() {
        let p = TilePyramid::from_xml(SAMPLE_OLD).unwrap();
        assert_eq!(p.levels.len(), 2);
        assert_eq!(p.max_size(), (5436, 4080));
    }
}
