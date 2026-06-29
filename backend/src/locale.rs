use axum::http::HeaderMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Zh,
}

impl Locale {
    pub fn from_headers(headers: &HeaderMap) -> Self {
        headers
            .get(axum::http::header::ACCEPT_LANGUAGE)
            .and_then(|value| value.to_str().ok())
            .map(Self::from_accept_language)
            .unwrap_or(Self::En)
    }

    pub fn from_accept_language(value: &str) -> Self {
        if value.to_lowercase().contains("zh") {
            Self::Zh
        } else {
            Self::En
        }
    }

    pub fn is_zh(self) -> bool {
        matches!(self, Self::Zh)
    }
}
