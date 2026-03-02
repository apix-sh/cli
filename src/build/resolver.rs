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
                ReferenceOr::Reference { reference: next_ref } => {
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
                    ReferenceOr::Reference { reference: next_ref } => {
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
