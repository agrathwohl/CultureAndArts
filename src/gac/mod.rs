mod discovery;
mod metadata;
mod palette;

pub use discovery::{discover, Discovered, GacClient, SEED_ASSETS};
pub use metadata::{Asset, Era};
pub use palette::{dominant_colors, palette_matches, parse_hex};
