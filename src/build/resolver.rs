use crate::error::ApixError;
use oas3::spec::{ObjectOrReference, ObjectSchema, PathItem, Spec as OpenAPI};
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
        if let Some(paths) = &spec.paths
            && let Some(item) = paths.get(&unescaped)
        {
            return Ok(item);
        }
    }

    Err(ApixError::Parse(format!(
        "Could not resolve path item reference: {}",
        reference
    )))
}

pub fn resolve_schema<'a>(
    reference: &'a str,
    spec: &'a OpenAPI,
    seen: &mut HashSet<&'a str>,
) -> Result<&'a ObjectSchema, ApixError> {
    if !seen.insert(reference) {
        return Err(ApixError::Parse(format!(
            "Circular schema reference detected: {}",
            reference
        )));
    }

    if reference.starts_with("#/components/schemas/") {
        let name = reference.trim_start_matches("#/components/schemas/");
        if let Some(components) = &spec.components
            && let Some(ref_or_item) = components.schemas.get(name)
        {
            match ref_or_item {
                ObjectOrReference::Object(item) => return Ok(item),
                ObjectOrReference::Ref {
                    ref_path: next_ref, ..
                } => {
                    return resolve_schema(next_ref, spec, seen);
                }
            }
        }
    }

    Err(ApixError::Parse(format!(
        "Could not resolve schema reference: {}",
        reference
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn resolve_path_item_unresolvable() {
        let spec = OpenAPI {
            openapi: "3.0.0".to_string(),
            info: oas3::spec::Info {
                title: "T".to_string(),
                version: "1".to_string(),
                summary: None,
                description: None,
                terms_of_service: None,
                contact: None,
                license: None,
                extensions: BTreeMap::new(),
            },
            servers: vec![],
            paths: None,
            components: None,
            security: vec![],
            tags: vec![],
            external_docs: None,
            webhooks: BTreeMap::new(),
            extensions: BTreeMap::new(),
        };

        let mut seen = HashSet::new();
        let err = resolve_path_item("#/paths/path_missing", &spec, &mut seen).unwrap_err();
        assert!(matches!(err, ApixError::Parse(msg) if msg.contains("Could not resolve")));
    }

    #[test]
    fn resolve_schema_circular() {
        let mut schemas = BTreeMap::new();
        schemas.insert(
            "schema_a".to_string(),
            ObjectOrReference::Ref {
                ref_path: "#/components/schemas/schema_b".to_string(),
                summary: None,
                description: None,
            },
        );
        schemas.insert(
            "schema_b".to_string(),
            ObjectOrReference::Ref {
                ref_path: "#/components/schemas/schema_a".to_string(),
                summary: None,
                description: None,
            },
        );

        let spec = OpenAPI {
            openapi: "3.0.0".to_string(),
            info: oas3::spec::Info {
                title: "T".to_string(),
                version: "1".to_string(),
                summary: None,
                description: None,
                terms_of_service: None,
                contact: None,
                license: None,
                extensions: BTreeMap::new(),
            },
            servers: vec![],
            paths: None,
            components: Some(oas3::spec::Components {
                schemas,
                ..Default::default()
            }),
            security: vec![],
            tags: vec![],
            external_docs: None,
            webhooks: BTreeMap::new(),
            extensions: BTreeMap::new(),
        };

        let mut seen = HashSet::new();
        let err = resolve_schema("#/components/schemas/schema_a", &spec, &mut seen).unwrap_err();
        assert!(matches!(err, ApixError::Parse(msg) if msg.contains("Circular schema reference")));
    }

    #[test]
    fn resolve_schema_unresolvable() {
        let spec = OpenAPI {
            openapi: "3.0.0".to_string(),
            info: oas3::spec::Info {
                title: "T".to_string(),
                version: "1".to_string(),
                summary: None,
                description: None,
                terms_of_service: None,
                contact: None,
                license: None,
                extensions: BTreeMap::new(),
            },
            servers: vec![],
            paths: None,
            components: None,
            security: vec![],
            tags: vec![],
            external_docs: None,
            webhooks: BTreeMap::new(),
            extensions: BTreeMap::new(),
        };

        let mut seen = HashSet::new();
        let err = resolve_schema("#/components/schemas/missing", &spec, &mut seen).unwrap_err();
        assert!(matches!(err, ApixError::Parse(msg) if msg.contains("Could not resolve")));
    }
}
