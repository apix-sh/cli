use crate::error::ApixError;
use openapiv3::{OpenAPI, PathItem, ReferenceOr, Schema};
use std::collections::HashSet;

pub fn resolve_path_item<'a>(
    reference: &'a str,
    spec: &'a OpenAPI,
    seen: &mut HashSet<&'a str>,
) -> Result<&'a PathItem, ApixError> {
    if !seen.insert(reference) {
        return Err(ApixError::Parse(format!(
            "Circular path reference detected: {}",
            reference
        )));
    }

    if reference.starts_with("#/paths/") {
        let path_key = reference.trim_start_matches("#/paths/");
        let unescaped = path_key.replace("~1", "/").replace("~0", "~");
        if let Some(ref_or_item) = spec.paths.paths.get(&unescaped) {
            match ref_or_item {
                ReferenceOr::Item(item) => return Ok(item),
                ReferenceOr::Reference {
                    reference: next_ref,
                } => {
                    return resolve_path_item(next_ref, spec, seen);
                }
            }
        }
    }

    Err(ApixError::Parse(format!(
        "Could not resolve path item reference: {}",
        reference
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_item_circular() {
        let spec = OpenAPI {
            openapi: "3.0.0".to_string(),
            info: Default::default(),
            servers: vec![],
            paths: openapiv3::Paths {
                paths: {
                    let mut map = indexmap::IndexMap::new();
                    map.insert("path_a".to_string(), ReferenceOr::Reference { reference: "#/paths/path_b".to_string() });
                    map.insert("path_b".to_string(), ReferenceOr::Reference { reference: "#/paths/path_a".to_string() });
                    map
                },
                ..Default::default()
            },
            components: None,
            security: None,
            tags: vec![],
            external_docs: None,
            extensions: indexmap::IndexMap::new(),
        };

        let mut seen = HashSet::new();
        let err = resolve_path_item("#/paths/path_a", &spec, &mut seen).unwrap_err();
        assert!(matches!(err, ApixError::Parse(msg) if msg.contains("Circular path reference")));
    }

    #[test]
    fn resolve_path_item_unresolvable() {
        let spec = OpenAPI {
            openapi: "3.0.0".to_string(),
            info: Default::default(),
            servers: vec![],
            paths: Default::default(),
            components: None,
            security: None,
            tags: vec![],
            external_docs: None,
            extensions: indexmap::IndexMap::new(),
        };

        let mut seen = HashSet::new();
        let err = resolve_path_item("#/paths/path_missing", &spec, &mut seen).unwrap_err();
        assert!(matches!(err, ApixError::Parse(msg) if msg.contains("Could not resolve")));
    }

    #[test]
    fn resolve_schema_circular() {
        let mut schemas = indexmap::IndexMap::new();
        schemas.insert("schema_a".to_string(), ReferenceOr::Reference { reference: "#/components/schemas/schema_b".to_string() });
        schemas.insert("schema_b".to_string(), ReferenceOr::Reference { reference: "#/components/schemas/schema_a".to_string() });

        let spec = OpenAPI {
            openapi: "3.0.0".to_string(),
            info: Default::default(),
            servers: vec![],
            paths: Default::default(),
            components: Some(openapiv3::Components {
                schemas,
                ..Default::default()
            }),
            security: None,
            tags: vec![],
            external_docs: None,
            extensions: indexmap::IndexMap::new(),
        };

        let mut seen = HashSet::new();
        let err = resolve_schema("#/components/schemas/schema_a", &spec, &mut seen).unwrap_err();
        assert!(matches!(err, ApixError::Parse(msg) if msg.contains("Circular schema reference")));
    }

    #[test]
    fn resolve_schema_unresolvable() {
        let spec = OpenAPI {
            openapi: "3.0.0".to_string(),
            info: Default::default(),
            servers: vec![],
            paths: Default::default(),
            components: None,
            security: None,
            tags: vec![],
            external_docs: None,
            extensions: indexmap::IndexMap::new(),
        };

        let mut seen = HashSet::new();
        let err = resolve_schema("#/components/schemas/missing", &spec, &mut seen).unwrap_err();
        assert!(matches!(err, ApixError::Parse(msg) if msg.contains("Could not resolve")));
    }
}

pub fn resolve_schema<'a>(
    reference: &'a str,
    spec: &'a OpenAPI,
    seen: &mut HashSet<&'a str>,
) -> Result<&'a Schema, ApixError> {
    if !seen.insert(reference) {
        return Err(ApixError::Parse(format!(
            "Circular schema reference detected: {}",
            reference
        )));
    }

    if reference.starts_with("#/components/schemas/") {
        let name = reference.trim_start_matches("#/components/schemas/");
        if let Some(components) = &spec.components {
            if let Some(ref_or_item) = components.schemas.get(name) {
                match ref_or_item {
                    ReferenceOr::Item(item) => return Ok(item),
                    ReferenceOr::Reference {
                        reference: next_ref,
                    } => {
                        return resolve_schema(next_ref, spec, seen);
                    }
                }
            }
        }
    }

    Err(ApixError::Parse(format!(
        "Could not resolve schema reference: {}",
        reference
    )))
}
