use crate::error::ApixError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Frontmatter {
    pub method: String,
    pub url: String,
    pub auth: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TypeFrontmatter {
    pub r#type: String,
}

pub fn extract_frontmatter<T: DeserializeOwned>(input: &str) -> Result<(T, &str), ApixError> {
    let mut parts = input.splitn(3, "---");
    let _before = parts.next().unwrap_or_default();
    let yaml = parts
        .next()
        .ok_or_else(|| ApixError::Parse("Missing frontmatter start delimiter".to_string()))?;
    let body = parts
        .next()
        .ok_or_else(|| ApixError::Parse("Missing frontmatter end delimiter".to_string()))?;

    let parsed = serde_yaml::from_str::<T>(yaml.trim())
        .map_err(|err| ApixError::Parse(format!("Invalid YAML frontmatter: {err}")))?;
    Ok((parsed, body.trim_start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_frontmatter() {
        let input = r#"---
method: GET
url: https://example.com/pets/{id}
auth: Bearer
content_type: application/json
---
# Pets
"#;

        let (fm, body) = extract_frontmatter::<Frontmatter>(input).expect("must parse");
        assert_eq!(fm.method, "GET");
        assert_eq!(fm.url, "https://example.com/pets/{id}");
        assert_eq!(fm.auth.as_deref(), Some("Bearer"));
        assert_eq!(fm.content_type.as_deref(), Some("application/json"));
        assert!(body.contains("# Pets"));
    }

    #[test]
    fn fails_on_missing_delimiter() {
        let input = "method: GET\nurl: https://example.com";
        let err = extract_frontmatter::<Frontmatter>(input).expect_err("must fail");
        match err {
            ApixError::Parse(msg) => assert!(msg.contains("delimiter")),
            _ => panic!("unexpected error variant"),
        }
    }
}
