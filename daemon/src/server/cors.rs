//! CORS for the localhost daemon.
//!
//! The daemon binds to 127.0.0.1, so the only callers are local processes
//! and browser extensions. CORS here is about letting extension background
//! pages (whose origin is `chrome-extension://...` or `moz-extension://...`)
//! reach the API without being blocked by the browser's same-origin policy.
//!
//! When `allowed_origins` is empty (the default) we allow **any** origin,
//! because the real security boundary is the localhost bind + path policy +
//! approval gate, not CORS. When origins are explicitly listed, we lock down
//! to that list.

use axum::http::{HeaderValue, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::models::Config;

pub fn layer(cfg: &Config) -> CorsLayer {
    let mut cors = CorsLayer::new()
        .allow_methods([Method::POST, Method::GET, Method::OPTIONS])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    if cfg.allowed_origins.is_empty() {
        // Open by default — localhost-only bind is the real boundary.
        cors = cors.allow_origin(AllowOrigin::any());
    } else {
        let origins: Vec<HeaderValue> = cfg
            .allowed_origins
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok())
            .collect();
        if origins.is_empty() {
            cors = cors.allow_origin(AllowOrigin::any());
        } else {
            cors = cors.allow_origin(AllowOrigin::list(origins));
        }
    }
    cors
}
