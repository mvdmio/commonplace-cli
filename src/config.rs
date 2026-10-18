use std::env;

const DEFAULT_HOST: &str = "https://commonplace.mvdm.io";

/// The Commonplace host: `COMMONPLACE_URL` when set, without a trailing slash.
pub fn host() -> String {
    match env::var("COMMONPLACE_URL") {
        Ok(url) if !url.trim().is_empty() => url.trim().trim_end_matches('/').to_string(),
        _ => DEFAULT_HOST.to_string(),
    }
}
