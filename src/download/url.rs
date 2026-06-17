use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use std::io::Write;

type HmacSha1 = Hmac<Sha1>;

const SIGN_KEY: [u8; 8] = [123, 43, 78, 35, 222, 44, 197, 197];

fn tile_path(base_url: &str) -> &str {
    base_url
        .splitn(4, '/')
        .nth(3)
        .expect("Google Arts base_url is malformed")
}

pub fn compute_tile_url(base_url: &str, token: &str, x: u32, y: u32, z: usize) -> String {
    let mut url = format!("{base_url}=x{x}-y{y}-z{z}-t");

    let mut sign_path: Vec<u8> = Vec::new();
    sign_path.extend_from_slice(tile_path(base_url).as_bytes());
    write!(sign_path, "=x{x}-y{y}-z{z}-t").unwrap();
    sign_path.extend_from_slice(token.as_bytes());

    let mut mac = HmacSha1::new_from_slice(&SIGN_KEY).expect("HMAC accepts any key length");
    mac.update(&sign_path);
    let digest = mac.finalize().into_bytes();

    url.push_str(&URL_SAFE_NO_PAD.encode(digest).replace('-', "_"));
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_known_flower_tile() {
        let url = compute_tile_url(
            "https://lh5.ggpht.com/D0sqZ0sJbzoQeYFoySoXLJqgLMfXhi8-gGVGRqD_UEYUqkqk9Eqdxx5NNaw",
            "mcOPEQJmk1514hP_dJkpwVwIhPU",
            0,
            0,
            7,
        );
        assert_eq!(
            url,
            "https://lh5.ggpht.com/D0sqZ0sJbzoQeYFoySoXLJqgLMfXhi8-gGVGRqD_UEYUqkqk9Eqdxx5NNaw=x0-y0-z7-tBJ_NeDnzAKjz3ZbOzN_uFRRIbS0"
        );
    }
}
