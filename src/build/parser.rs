use crate::error::ApixError;
use openapiv3::OpenAPI;

#[derive(Debug, Clone)]
pub struct ParsedSpec {
    pub openapi: OpenAPI,
    pub base_url: String,
    pub title: String,
    pub description: String,
    pub version: String,
}

pub fn parse_spec(source: &str) -> Result<ParsedSpec, ApixError> {
    let content = if source.starts_with("http://") || source.starts_with("https://") {
        ureq::get(source)
            .call()
            .map_err(|err| ApixError::Http(format!("Failed to fetch spec: {err}")))?
            .into_string()
            .map_err(|err| ApixError::Http(format!("Failed to read response body: {err}")))?
    } else {
        std::fs::read_to_string(source)?
    };

    let openapi: OpenAPI = match detect_format(source, &content) {
        SpecFormat::Yaml => serde_yaml::from_str(&content)
            .map_err(|err| ApixError::Parse(format!("Invalid YAML OpenAPI spec: {err}")))?,
        SpecFormat::Json => serde_json::from_str(&content)
            .map_err(|err| ApixError::Parse(format!("Invalid JSON OpenAPI spec: {err}")))?,
        SpecFormat::Unknown => serde_json::from_str(&content).or_else(|_| {
            serde_yaml::from_str(&content)
                .map_err(|err| ApixError::Parse(format!("Invalid OpenAPI spec (JSON/YAML): {err}")))
        })?,
    };

    let base_url = openapi
        .servers
        .first()
        .map(|s| s.url.clone())
        .unwrap_or_default();

    Ok(ParsedSpec {
        title: openapi.info.title.clone(),
        description: openapi.info.description.clone().unwrap_or_default(),
        version: openapi.info.version.clone(),
        base_url,
        openapi,
    })
}

#[derive(Debug, Clone, Copy)]
enum SpecFormat {
    Json,
    Yaml,
    Unknown,
}

fn detect_format(source: &str, content: &str) -> SpecFormat {
    if source.ends_with(".yaml") || source.ends_with(".yml") {
        return SpecFormat::Yaml;
    }
    if source.ends_with(".json") {
        return SpecFormat::Json;
    }
    let first = content.chars().find(|c| !c.is_whitespace());
    match first {
        Some('{') | Some('[') => SpecFormat::Json,
        Some(_) => SpecFormat::Unknown,
        None => SpecFormat::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_json_spec_file() {
        let spec = r#"{
  "openapi": "3.0.0",
  "info": { "title": "Pet API", "version": "v1" },
  "servers": [{ "url": "https://api.example.com" }],
  "paths": {}
}"#;
        let path =
            std::env::temp_dir().join(format!("apix-parser-test-{}.json", std::process::id()));
        std::fs::write(&path, spec).expect("write spec");

        let parsed = parse_spec(path.to_str().expect("path str")).expect("must parse");
        assert_eq!(parsed.title, "Pet API");
        assert_eq!(parsed.version, "v1");
        assert_eq!(parsed.base_url, "https://api.example.com");
        assert!(parsed.openapi.paths.paths.is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parses_minimal_yaml_spec_file() {
        let spec = r#"openapi: 3.0.0
info:
  title: Pet API
  version: v1
servers:
  - url: https://api.example.com
paths: {}"#;
        let path = std::env::temp_dir().join(format!("apix-parser-test-{}.yaml", std::process::id()));
        std::fs::write(&path, spec).expect("write spec");

        let parsed = parse_spec(path.to_str().expect("path str")).expect("must parse");
        assert_eq!(parsed.title, "Pet API");
        assert_eq!(parsed.version, "v1");
        assert_eq!(parsed.base_url, "https://api.example.com");
        assert!(parsed.openapi.paths.paths.is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parses_invalid_spec_fails() {
        let spec = r#"invalid JSON or YAML format"#;
        let path = std::env::temp_dir().join(format!("apix-parser-invalid-{}.txt", std::process::id()));
        std::fs::write(&path, spec).expect("write spec");

        let res = parse_spec(path.to_str().expect("path str"));
        assert!(res.is_err());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_detect_format() {
        assert!(matches!(detect_format("spec.yaml", ""), SpecFormat::Yaml));
        assert!(matches!(detect_format("spec.yml", ""), SpecFormat::Yaml));
        assert!(matches!(detect_format("spec.json", ""), SpecFormat::Json));

        assert!(matches!(detect_format("spec.txt", " { "), SpecFormat::Json));
        assert!(matches!(detect_format("spec.txt", " [ "), SpecFormat::Json));
        assert!(matches!(detect_format("spec.txt", " openapi: 3.0.0 "), SpecFormat::Unknown));
        assert!(matches!(detect_format("spec.txt", ""), SpecFormat::Unknown));
    }
}
