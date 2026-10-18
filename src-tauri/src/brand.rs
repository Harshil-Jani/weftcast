//! The product name and identity, read from the repo's brand.json at compile time.
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brand {
    pub name: String,
    pub slug: String,
    pub identifier: String,
    pub tagline: String,
    pub accent: String,
    pub repo: String,
}

pub static BRAND: Lazy<Brand> = Lazy::new(|| {
    serde_json::from_str(include_str!("../../brand.json")).expect("brand.json is valid")
});

#[cfg(test)]
mod tests {
    #[test]
    fn brand_loads() {
        let b = &*super::BRAND;
        assert!(!b.name.is_empty() && !b.slug.is_empty() && b.identifier.contains('.'));
    }
}
