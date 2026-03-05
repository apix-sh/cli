use crate::error::ApixError;
use openapiv3::{APIKeyLocation, Components, OpenAPI, ReferenceOr, SecurityScheme};

#[derive(Debug, Clone)]
pub struct ParsedSpec {
    pub openapi: OpenAPI,
    pub base_url: String,
    pub title: String,
    pub description: String,
    pub version: String,
    pub auth: String,
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

    let auth = match &openapi.security {
        Some(reqs) if !reqs.is_empty() => {
            format_security_schemes(reqs, &openapi.components)
        }
        _ => "none".to_string(),
    };

    Ok(ParsedSpec {
        title: openapi.info.title.clone(),
        description: openapi.info.description.clone().unwrap_or_default(),
        version: openapi.info.version.clone(),
        base_url,
        auth,
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

pub fn format_security_schemes(
    requirements: &[indexmap::IndexMap<String, Vec<String>>],
    components: &Option<Components>,
) -> String {
    let parts: Vec<String> = requirements
        .iter()
        .filter_map(|req| {
            let scheme_parts: Vec<String> = req
                .keys()
                .map(|name| format_single_scheme(name, components))
                .collect();
            if scheme_parts.is_empty() {
                None
            } else {
                Some(scheme_parts.join(" + "))
            }
        })
        .collect();

    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(" | ")
    }
}

fn format_single_scheme(name: &str, components: &Option<Components>) -> String {
    let Some(comps) = components else {
        return name.to_string();
    };

    let Some(scheme_ref) = comps.security_schemes.get(name) else {
        return name.to_string();
    };

    match scheme_ref {
        ReferenceOr::Reference { .. } => name.to_string(),
        ReferenceOr::Item(scheme) => match scheme {
            SecurityScheme::HTTP {
                scheme,
                bearer_format,
                ..
            } => {
                if let Some(bf) = bearer_format {
                    format!("{scheme} ({bf})")
                } else {
                    scheme.to_string()
                }
            }
            SecurityScheme::APIKey {
                location,
                name: key_name,
                ..
            } => {
                let loc = match location {
                    APIKeyLocation::Query => "query",
                    APIKeyLocation::Header => "header",
                    APIKeyLocation::Cookie => "cookie",
                };
                format!("apiKey ({loc}: {key_name})")
            }
            SecurityScheme::OAuth2 { .. } => "oauth2".to_string(),
            SecurityScheme::OpenIDConnect { .. } => "openIdConnect".to_string(),
        },
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

    #[test]
    fn format_security_bearer() {
        let components = Some(openapiv3::Components {
            security_schemes: indexmap::indexmap! {
                "bearerAuth".to_string() => ReferenceOr::Item(SecurityScheme::HTTP {
                    scheme: "bearer".to_string(),
                    bearer_format: None,
                    description: None,
                    extensions: Default::default(),
                }),
            },
            ..Default::default()
        });
        let reqs = vec![indexmap::indexmap! { "bearerAuth".to_string() => vec![] }];
        assert_eq!(format_security_schemes(&reqs, &components), "bearer");
    }

    #[test]
    fn format_security_bearer_with_format() {
        let components = Some(openapiv3::Components {
            security_schemes: indexmap::indexmap! {
                "bearerAuth".to_string() => ReferenceOr::Item(SecurityScheme::HTTP {
                    scheme: "bearer".to_string(),
                    bearer_format: Some("JWT".to_string()),
                    description: None,
                    extensions: Default::default(),
                }),
            },
            ..Default::default()
        });
        let reqs = vec![indexmap::indexmap! { "bearerAuth".to_string() => vec![] }];
        assert_eq!(format_security_schemes(&reqs, &components), "bearer (JWT)");
    }

    #[test]
    fn format_security_api_key() {
        let components = Some(openapiv3::Components {
            security_schemes: indexmap::indexmap! {
                "apiKeyAuth".to_string() => ReferenceOr::Item(SecurityScheme::APIKey {
                    location: APIKeyLocation::Header,
                    name: "X-API-KEY".to_string(),
                    description: None,
                    extensions: Default::default(),
                }),
            },
            ..Default::default()
        });
        let reqs = vec![indexmap::indexmap! { "apiKeyAuth".to_string() => vec![] }];
        assert_eq!(
            format_security_schemes(&reqs, &components),
            "apiKey (header: X-API-KEY)"
        );
    }

    #[test]
    fn format_security_oauth2() {
        let components = Some(openapiv3::Components {
            security_schemes: indexmap::indexmap! {
                "oauth".to_string() => ReferenceOr::Item(SecurityScheme::OAuth2 {
                    flows: openapiv3::OAuth2Flows::default(),
                    description: None,
                    extensions: Default::default(),
                }),
            },
            ..Default::default()
        });
        let reqs = vec![indexmap::indexmap! { "oauth".to_string() => vec![] }];
        assert_eq!(format_security_schemes(&reqs, &components), "oauth2");
    }

    #[test]
    fn format_security_multiple_alternatives() {
        let components = Some(openapiv3::Components {
            security_schemes: indexmap::indexmap! {
                "bearerAuth".to_string() => ReferenceOr::Item(SecurityScheme::HTTP {
                    scheme: "bearer".to_string(),
                    bearer_format: None,
                    description: None,
                    extensions: Default::default(),
                }),
                "apiKeyAuth".to_string() => ReferenceOr::Item(SecurityScheme::APIKey {
                    location: APIKeyLocation::Header,
                    name: "X-API-KEY".to_string(),
                    description: None,
                    extensions: Default::default(),
                }),
            },
            ..Default::default()
        });
        let reqs = vec![
            indexmap::indexmap! { "bearerAuth".to_string() => vec![] },
            indexmap::indexmap! { "apiKeyAuth".to_string() => vec![] },
        ];
        assert_eq!(
            format_security_schemes(&reqs, &components),
            "bearer | apiKey (header: X-API-KEY)"
        );
    }

    #[test]
    fn format_security_empty_requirements() {
        let components = Some(openapiv3::Components::default());
        let reqs: Vec<indexmap::IndexMap<String, Vec<String>>> = vec![];
        assert_eq!(format_security_schemes(&reqs, &components), "none");
    }

    #[test]
    fn format_security_unknown_scheme_falls_back_to_name() {
        let components = Some(openapiv3::Components::default());
        let reqs = vec![indexmap::indexmap! { "unknownScheme".to_string() => vec![] }];
        assert_eq!(
            format_security_schemes(&reqs, &components),
            "unknownScheme"
        );
    }
}
