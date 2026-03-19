use crate::error::ApixError;
use oas3::spec::{Components, ObjectOrReference, SecurityScheme, Spec};
use regex::Regex;
use serde_json::{Map, Value};
use url::Url;

#[derive(Debug, Clone)]
pub struct ParsedSpec {
    pub spec: Spec,
    pub base_url: String,
    pub title: String,
    pub description: String,
    pub version: String,
    pub auth: String,
    pub tags: Vec<String>,
}

pub fn parse_spec(source: &str) -> Result<ParsedSpec, ApixError> {
    let raw = if source.starts_with("http://") || source.starts_with("https://") {
        let resp = ureq::get(source)
            .call()
            .map_err(|err| ApixError::Http(format!("Failed to fetch spec: {err}")))?;
        crate::http::read_response(resp)?
    } else {
        std::fs::read_to_string(source)?
    };

    let spec = parse_spec_with_compat(&raw).map_err(|err| {
        ApixError::Parse(format!(
            "Invalid OpenAPI spec (JSON/YAML), including compat retry: {err}"
        ))
    })?;

    let base_url = spec
        .servers
        .first()
        .map(|s| s.url.clone())
        .unwrap_or_default();

    let auth = if spec.security.is_empty() {
        "none".to_string()
    } else {
        format_security_schemes(&spec.security, &spec.components)
    };

    let tags = spec.tags.iter().map(|t| t.name.clone()).collect();
    Ok(ParsedSpec {
        title: spec.info.title.clone(),
        description: spec.info.description.clone().unwrap_or_default(),
        version: spec.info.version.clone(),
        base_url,
        auth,
        tags,
        spec,
    })
}

fn parse_spec_with_compat(raw: &str) -> Result<Spec, String> {
    // Prefer parsing the original content first, then fall back to compat sanitization only if needed.
    match parse_spec_content(raw) {
        Ok(spec) => Ok(spec),
        Err(initial_err) => {
            let sanitized = sanitize_spec_for_compat(raw);
            parse_spec_content(&sanitized).map_err(|sanitized_err| {
                format!("initial parse: {initial_err}; sanitized parse: {sanitized_err}")
            })
        }
    }
}

fn parse_spec_content(content: &str) -> Result<Spec, String> {
    oas3::from_json(content.to_string())
        .or_else(|json_err| {
            oas3::from_yaml(content.to_string())
                .map_err(|yaml_err| format!("json error: {json_err}; yaml error: {yaml_err}"))
        })
        .map_err(|err| err.to_string())
}

/// Sanitizes the raw spec text to handle common compatibility issues between OpenAPI 3.0 and 3.1,
/// or specific parser limitations.
fn sanitize_spec_for_compat(content: &str) -> String {
    // 1. YAML 1.2 natively supports only 64-bit integers. Some OpenAPI specs (e.g., OpenAI's) use
    // integers like `-9223372036854776000` in `minimum`/`maximum` fields as a "no practical limit"
    // sentinel. These values fall just outside `i64` range, so `serde_yaml` silently converts them
    // to floats, which then fail to deserialize into schema bounds typed as integers.
    let re_int = Regex::new(r"(?m)(:\s*)(-?\d{15,})").expect("valid regex");
    let content = re_int.replace_all(content, |caps: &regex::Captures| {
        let prefix = &caps[1];
        let num_str = &caps[2];
        // Try to parse as i128 first (to detect overflow vs. i64).
        if let Ok(n) = num_str.parse::<i128>() {
            if n > i64::MAX as i128 {
                return format!("{}{}", prefix, i64::MAX);
            } else if n < i64::MIN as i128 {
                return format!("{}{}", prefix, i64::MIN);
            }
        } else {
            // Doesn't even fit in i128: clamp by sign.
            if num_str.starts_with('-') {
                return format!("{}{}", prefix, i64::MIN);
            } else {
                return format!("{}{}", prefix, i64::MAX);
            }
        }
        // Within i64 range — leave unchanged.
        format!("{}{}", prefix, num_str)
    });

    // 2. OpenAPI 3.1 (JSON Schema 2020-12) changed `exclusiveMinimum` and `exclusiveMaximum`
    // from booleans to numbers. Many 3.0 specs use booleans. We rename them to `x-` prefixed
    // extensions to allow the spec to parse while losing only the strictness of the constraint.
    let re_exc = Regex::new(r"(?m)(exclusive(?:Minimum|Maximum))(\s*:\s*)(true|false)")
        .expect("valid regex");
    let content = re_exc
        .replace_all(&content, |caps: &regex::Captures| {
            format!("x-{}{}{}", &caps[1], &caps[2], &caps[3])
        })
        .into_owned();

    if let Some(updated) = sanitize_relative_oauth_urls(&content) {
        updated
    } else {
        content
    }
}

fn sanitize_relative_oauth_urls(content: &str) -> Option<String> {
    let mut doc: Value = serde_json::from_str(content)
        .or_else(|_| serde_yaml::from_str(content))
        .ok()?;

    let base = doc
        .get("servers")
        .and_then(|v| v.as_array())
        .and_then(|servers| servers.first())
        .and_then(|server| server.get("url"))
        .and_then(|url| url.as_str())
        .and_then(|url| Url::parse(url).ok())?;

    let mut changed = false;
    if let Some(schemes) = doc
        .get_mut("components")
        .and_then(Value::as_object_mut)
        .and_then(|components| components.get_mut("securitySchemes"))
        .and_then(Value::as_object_mut)
    {
        for scheme in schemes.values_mut() {
            let Some(scheme_obj) = scheme.as_object_mut() else {
                continue;
            };
            if scheme_obj.get("type").and_then(Value::as_str) == Some("oauth2") {
                if let Some(flows) = scheme_obj.get_mut("flows").and_then(Value::as_object_mut) {
                    for flow in flows.values_mut() {
                        let Some(flow_obj) = flow.as_object_mut() else {
                            continue;
                        };
                        changed |= absolutize_url_field(flow_obj, "authorizationUrl", &base);
                        changed |= absolutize_url_field(flow_obj, "tokenUrl", &base);
                        changed |= absolutize_url_field(flow_obj, "refreshUrl", &base);
                    }
                }
            } else if scheme_obj.get("type").and_then(Value::as_str) == Some("openIdConnect") {
                changed |= absolutize_url_field(scheme_obj, "openIdConnectUrl", &base);
            }
        }
    }

    if !changed {
        return None;
    }

    serde_json::to_string(&doc).ok()
}

fn absolutize_url_field(obj: &mut Map<String, Value>, key: &str, base: &Url) -> bool {
    let Some(raw) = obj.get(key).and_then(Value::as_str) else {
        return false;
    };
    if Url::parse(raw).is_ok() {
        return false;
    }
    if !raw.starts_with('/') {
        return false;
    }
    let Ok(abs) = base.join(raw) else {
        return false;
    };
    obj.insert(key.to_string(), Value::String(abs.to_string()));
    true
}

pub fn format_security_schemes(
    requirements: &[oas3::spec::SecurityRequirement],
    components: &Option<Components>,
) -> String {
    let parts: Vec<String> = requirements
        .iter()
        .filter_map(|req| {
            let scheme_parts: Vec<String> = req
                .0
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
        ObjectOrReference::Ref { .. } => name.to_string(),
        ObjectOrReference::Object(scheme) => match scheme {
            SecurityScheme::Http {
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
            SecurityScheme::ApiKey {
                location,
                name: key_name,
                ..
            } => {
                format!("apiKey ({location}: {key_name})")
            }
            SecurityScheme::OAuth2 { .. } => "oauth2".to_string(),
            SecurityScheme::OpenIdConnect { .. } => "openIdConnect".to_string(),
            SecurityScheme::MutualTls { .. } => "mutualTls".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

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
        assert!(
            parsed
                .spec
                .paths
                .as_ref()
                .map(|p| p.is_empty())
                .unwrap_or(true)
        );

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
        let path =
            std::env::temp_dir().join(format!("apix-parser-test-{}.yaml", std::process::id()));
        std::fs::write(&path, spec).expect("write spec");

        let parsed = parse_spec(path.to_str().expect("path str")).expect("must parse");
        assert_eq!(parsed.title, "Pet API");
        assert_eq!(parsed.version, "v1");
        assert_eq!(parsed.base_url, "https://api.example.com");
        assert!(
            parsed
                .spec
                .paths
                .as_ref()
                .map(|p| p.is_empty())
                .unwrap_or(true)
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parses_invalid_spec_fails() {
        let spec = r#"invalid JSON or YAML format"#;
        let path =
            std::env::temp_dir().join(format!("apix-parser-invalid-{}.txt", std::process::id()));
        std::fs::write(&path, spec).expect("write spec");

        let res = parse_spec(path.to_str().expect("path str"));
        assert!(res.is_err());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn format_security_bearer() {
        let mut security_schemes = BTreeMap::new();
        security_schemes.insert(
            "bearerAuth".to_string(),
            ObjectOrReference::Object(SecurityScheme::Http {
                scheme: "bearer".to_string(),
                bearer_format: None,
                description: None,
            }),
        );
        let components = Some(Components {
            security_schemes,
            ..Default::default()
        });

        let mut req = oas3::spec::SecurityRequirement(BTreeMap::new());
        req.0.insert("bearerAuth".to_string(), vec![]);
        let reqs = vec![req];
        assert_eq!(format_security_schemes(&reqs, &components), "bearer");
    }

    #[test]
    fn format_security_api_key() {
        let mut security_schemes = BTreeMap::new();
        security_schemes.insert(
            "apiKeyAuth".to_string(),
            ObjectOrReference::Object(SecurityScheme::ApiKey {
                location: "header".to_string(),
                name: "X-API-KEY".to_string(),
                description: None,
            }),
        );
        let components = Some(Components {
            security_schemes,
            ..Default::default()
        });

        let mut req = oas3::spec::SecurityRequirement(BTreeMap::new());
        req.0.insert("apiKeyAuth".to_string(), vec![]);
        let reqs = vec![req];
        assert_eq!(
            format_security_schemes(&reqs, &components),
            "apiKey (header: X-API-KEY)"
        );
    }

    #[test]
    fn format_security_multiple_alternatives() {
        let mut security_schemes = BTreeMap::new();
        security_schemes.insert(
            "bearerAuth".to_string(),
            ObjectOrReference::Object(SecurityScheme::Http {
                scheme: "bearer".to_string(),
                bearer_format: None,
                description: None,
            }),
        );
        security_schemes.insert(
            "apiKeyAuth".to_string(),
            ObjectOrReference::Object(SecurityScheme::ApiKey {
                location: "header".to_string(),
                name: "X-API-KEY".to_string(),
                description: None,
            }),
        );
        let components = Some(Components {
            security_schemes,
            ..Default::default()
        });

        let mut req1 = oas3::spec::SecurityRequirement(BTreeMap::new());
        req1.0.insert("bearerAuth".to_string(), vec![]);
        let mut req2 = oas3::spec::SecurityRequirement(BTreeMap::new());
        req2.0.insert("apiKeyAuth".to_string(), vec![]);

        let reqs = vec![req1, req2];
        assert_eq!(
            format_security_schemes(&reqs, &components),
            "bearer | apiKey (header: X-API-KEY)"
        );
    }

    #[test]
    fn format_security_oauth2() {
        let mut security_schemes = BTreeMap::new();
        security_schemes.insert(
            "oauth".to_string(),
            ObjectOrReference::Object(SecurityScheme::OAuth2 {
                flows: oas3::spec::Flows::default(),
                description: None,
            }),
        );
        let components = Some(Components {
            security_schemes,
            ..Default::default()
        });
        let mut req = oas3::spec::SecurityRequirement(BTreeMap::new());
        req.0.insert("oauth".to_string(), vec![]);
        let reqs = vec![req];
        assert_eq!(format_security_schemes(&reqs, &components), "oauth2");
    }

    #[test]
    fn parses_spec_with_large_integer() {
        let spec = r#"openapi: 3.0.0
info:
  title: Large Int Test
  version: v1
paths:
  /test:
    post:
      responses:
        '200':
          description: OK
          content:
            application/json:
              schema:
                type: integer
                minimum: -9223372036854776000
                maximum: 9223372036854776000
"#;
        let path =
            std::env::temp_dir().join(format!("apix-parser-large-int-{}.yaml", std::process::id()));
        std::fs::write(&path, spec).expect("write spec");

        let res = parse_spec(path.to_str().expect("path str"));
        assert!(
            res.is_ok(),
            "Should parse spec with large integers: {:?}",
            res.err()
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn sanitize_spec_for_compat_clamps_out_of_range() {
        let input = "minimum: -9223372036854776000\nmaximum: 9223372036854776000\nnormal: 12345";
        let output = sanitize_spec_for_compat(input);
        assert!(output.contains(&format!("minimum: {}", i64::MIN)));
        assert!(output.contains(&format!("maximum: {}", i64::MAX)));
        assert!(
            output.contains("normal: 12345"),
            "in-range values must be unchanged"
        );
    }

    #[test]
    fn sanitize_spec_for_compat_leaves_normal_values_intact() {
        let input = "seed: 42\nlimit: 100\noffset: -500";
        let output = sanitize_spec_for_compat(input);
        assert_eq!(input, output);
    }

    #[test]
    fn sanitize_spec_for_compat_renames_exclusive_booleans() {
        let input = "minimum: 0\nexclusiveMinimum: true\nmaximum: 10\nexclusiveMaximum: false";
        let output = sanitize_spec_for_compat(input);
        assert!(output.contains("x-exclusiveMinimum: true"));
        assert!(output.contains("x-exclusiveMaximum: false"));
        assert!(!output.contains("\nexclusiveMinimum: true"));
        assert!(!output.contains("\nexclusiveMaximum: false"));
    }

    #[test]
    fn parses_openapi_3_1_type_array() {
        let spec = r#"{
  "openapi": "3.1.0",
  "info": { "title": "3.1 API", "version": "v1" },
  "paths": {
    "/test": {
      "get": {
        "parameters": [
          {
            "name": "id",
            "in": "query",
            "schema": { "type": ["string", "null"] }
          }
        ],
        "responses": {
          "200": { "description": "OK" }
        }
      }
    }
  }
}"#;
        let path = std::env::temp_dir().join(format!("apix-parser-31-{}.json", std::process::id()));
        std::fs::write(&path, spec).expect("write spec");

        let parsed = parse_spec(path.to_str().expect("path str")).expect("must parse 3.1");
        assert_eq!(parsed.title, "3.1 API");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn sanitize_spec_for_compat_absolutizes_relative_oauth_urls() {
        let input = r#"{
  "openapi": "3.0.0",
  "info": { "title": "t", "version": "v1" },
  "servers": [{ "url": "https://api.example.com" }],
  "paths": {},
  "components": {
    "securitySchemes": {
      "Oauth2": {
        "type": "oauth2",
        "flows": {
          "clientCredentials": {
            "tokenUrl": "/v1/oauth2/token",
            "scopes": {}
          }
        }
      }
    }
  }
}"#;
        let output = sanitize_spec_for_compat(input);
        assert!(
            output.contains(r#""tokenUrl":"https://api.example.com/v1/oauth2/token""#),
            "expected absolute tokenUrl, got: {output}"
        );
    }

    #[test]
    fn parses_spec_with_relative_oauth_token_url() {
        let spec = r#"{
  "openapi": "3.0.0",
  "info": { "title": "OAuth Relative URL", "version": "v1" },
  "servers": [{ "url": "https://api.example.com" }],
  "paths": {},
  "components": {
    "securitySchemes": {
      "Oauth2": {
        "type": "oauth2",
        "flows": {
          "clientCredentials": {
            "tokenUrl": "/v1/oauth2/token",
            "scopes": {}
          }
        }
      }
    }
  }
}"#;
        let path = std::env::temp_dir().join(format!(
            "apix-parser-relative-oauth-url-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, spec).expect("write spec");

        let parsed = parse_spec(path.to_str().expect("path str"));
        assert!(
            parsed.is_ok(),
            "relative OAuth tokenUrl should be sanitized and parse successfully: {:?}",
            parsed.err()
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parses_paypal_relative_oauth_fixture() {
        let spec_path = fixture("paypal_relative_oauth.json");
        let parsed = parse_spec(spec_path.to_str().expect("path str"));
        assert!(
            parsed.is_ok(),
            "PayPal-style relative oauth tokenUrl should parse: {:?}",
            parsed.err()
        );
    }

    #[test]
    fn sanitize_spec_for_compat_does_not_rewrite_non_root_relative_oauth_urls() {
        let input = r#"{
  "openapi": "3.0.0",
  "info": { "title": "No Rewrite", "version": "v1" },
  "servers": [{ "url": "https://api.example.com" }],
  "paths": {},
  "components": {
    "securitySchemes": {
      "Oauth2": {
        "type": "oauth2",
        "flows": {
          "clientCredentials": {
            "tokenUrl": "v1/oauth2/token",
            "scopes": {}
          }
        }
      }
    }
  }
}"#;
        let output = sanitize_spec_for_compat(input);
        assert_eq!(
            output, input,
            "non-root-relative URL must remain unchanged"
        );
    }

    #[test]
    fn sanitize_spec_for_compat_rewrites_only_relative_urls_in_mixed_flows() {
        let input = r#"{
  "openapi": "3.0.0",
  "info": { "title": "Mixed URLs", "version": "v1" },
  "servers": [{ "url": "https://api.example.com/base" }],
  "paths": {},
  "components": {
    "securitySchemes": {
      "Oauth2": {
        "type": "oauth2",
        "flows": {
          "authorizationCode": {
            "authorizationUrl": "/oauth/authorize",
            "tokenUrl": "https://idp.example.com/oauth/token",
            "refreshUrl": "/oauth/refresh",
            "scopes": {}
          },
          "clientCredentials": {
            "tokenUrl": "/oauth/token2",
            "scopes": {}
          }
        }
      }
    }
  }
}"#;
        let output = sanitize_spec_for_compat(input);
        let doc: Value = serde_json::from_str(&output).expect("valid json");
        let auth_code = &doc["components"]["securitySchemes"]["Oauth2"]["flows"]["authorizationCode"];
        let client_credentials =
            &doc["components"]["securitySchemes"]["Oauth2"]["flows"]["clientCredentials"];

        assert_eq!(
            auth_code["authorizationUrl"].as_str(),
            Some("https://api.example.com/oauth/authorize")
        );
        assert_eq!(
            auth_code["tokenUrl"].as_str(),
            Some("https://idp.example.com/oauth/token")
        );
        assert_eq!(
            auth_code["refreshUrl"].as_str(),
            Some("https://api.example.com/oauth/refresh")
        );
        assert_eq!(
            client_credentials["tokenUrl"].as_str(),
            Some("https://api.example.com/oauth/token2")
        );
    }
}
