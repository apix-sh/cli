use crate::error::ApixError;
use askama::Template;
use openapiv3::{ReferenceOr, Schema, SchemaKind, Type};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

use super::parser::ParsedSpec;

#[derive(Debug)]
struct PropertyRow {
    name: String,
    required: String,
    prop_type: String,
    description: String,
}

#[derive(Template)]
#[template(path = "type.md")]
struct TypeTemplate<'a> {
    schema_type: &'a str,
    name: &'a str,
    description: &'a str,
    properties: &'a [PropertyRow],
}

pub fn generate_types(
    parsed: &ParsedSpec,
    out_root: &Path,
    namespace: &str,
) -> Result<usize, ApixError> {
    let mut count = 0usize;
    let types_dir = out_root.join("_types");
    std::fs::create_dir_all(&types_dir)?;

    let Some(components) = &parsed.openapi.components else {
        return Ok(0);
    };

    for (name, schema_ref) in &components.schemas {
        let (schema_type, description, properties) =
            schema_details(schema_ref, namespace, &parsed.version);

        let tpl = TypeTemplate {
            schema_type: &schema_type,
            name,
            description: &description,
            properties: &properties,
        };

        let rendered = tpl.render().map_err(|err| {
            ApixError::Parse(format!("Failed to render type template {name}: {err}"))
        })?;

        std::fs::write(types_dir.join(format!("{name}.md")), rendered)?;
        count += 1;
    }

    Ok(count)
}

fn schema_details(
    schema_ref: &ReferenceOr<Schema>,
    namespace: &str,
    version: &str,
) -> (String, String, Vec<PropertyRow>) {
    match schema_ref {
        ReferenceOr::Reference { reference } => (
            "reference".to_string(),
            format!("Reference to `{reference}`"),
            Vec::new(),
        ),
        ReferenceOr::Item(schema) => {
            let schema_type = kind_to_string(&schema.schema_kind);
            let mut description = schema.schema_data.description.clone().unwrap_or_default();
            if let Some(variants) = variant_links(schema, namespace, version) {
                if !description.is_empty() {
                    description.push_str("\n\n");
                }
                description.push_str(&variants);
            }
            let properties = collect_properties(schema, namespace, version);
            (schema_type, description, properties)
        }
    }
}

fn collect_properties(schema: &Schema, namespace: &str, version: &str) -> Vec<PropertyRow> {
    match &schema.schema_kind {
        SchemaKind::Type(Type::Object(obj)) => {
            let mut rows = Vec::new();
            let required: HashSet<&str> = obj.required.iter().map(String::as_str).collect();
            for (prop_name, prop_schema) in &obj.properties {
                let (ptype, desc) = prop_type_and_description(prop_schema, namespace, version);
                rows.push(PropertyRow {
                    name: prop_name.to_string(),
                    required: if required.contains(prop_name.as_str()) {
                        "Yes".to_string()
                    } else {
                        "No".to_string()
                    },
                    prop_type: ptype,
                    description: desc,
                });
            }
            rows
        }
        SchemaKind::AllOf { all_of } => {
            let mut rows = Vec::new();
            for item in all_of {
                if let ReferenceOr::Item(inner) = item {
                    rows.extend(collect_properties(inner, namespace, version));
                }
            }
            rows
        }
        _ => Vec::new(),
    }
}

fn prop_type_and_description(
    prop_schema: &ReferenceOr<Box<Schema>>,
    _namespace: &str,
    _version: &str,
) -> (String, String) {
    match prop_schema {
        ReferenceOr::Reference { reference } => {
            let name = reference.rsplit('/').next().unwrap_or(reference);
            (
                format!("[{name}]({name}.md)"),
                String::new(),
            )
        }
        ReferenceOr::Item(inner) => {
            let ptype = kind_to_string(&inner.schema_kind);
            let mut description = inner.schema_data.description.clone().unwrap_or_default();
            if let Some(enum_values) = string_enum_values(inner) {
                if !description.is_empty() {
                    description.push(' ');
                }
                description.push_str(&format!("Allowed values: {}", enum_values.join(", ")));
            }
            (ptype, description)
        }
    }
}

fn string_enum_values(schema: &Schema) -> Option<Vec<String>> {
    match &schema.schema_kind {
        SchemaKind::Type(Type::String(st)) => {
            let vals: Vec<String> = st.enumeration.iter().filter_map(|v| v.clone()).collect();
            if vals.is_empty() { None } else { Some(vals) }
        }
        _ => None,
    }
}

fn variant_links(schema: &Schema, _namespace: &str, _version: &str) -> Option<String> {
    let refs: Vec<String> = match &schema.schema_kind {
        SchemaKind::OneOf { one_of } => one_of
            .iter()
            .filter_map(ref_name)
            .map(|name| format!("- [{name}]({name}.md)"))
            .collect(),
        SchemaKind::AnyOf { any_of } => any_of
            .iter()
            .filter_map(ref_name)
            .map(|name| format!("- [{name}]({name}.md)"))
            .collect(),
        _ => return None,
    };

    if refs.is_empty() {
        None
    } else {
        Some(format!("Variants:\n{}", refs.join("\n")))
    }
}

fn ref_name(item: &ReferenceOr<Schema>) -> Option<String> {
    match item {
        ReferenceOr::Reference { reference } => Some(
            reference
                .rsplit('/')
                .next()
                .unwrap_or(reference)
                .to_string(),
        ),
        ReferenceOr::Item(_) => None,
    }
}

pub(crate) fn kind_to_string(kind: &SchemaKind) -> String {
    match kind {
        SchemaKind::Type(ty) => match ty {
            Type::String(_) => "string".to_string(),
            Type::Number(_) => "number".to_string(),
            Type::Integer(_) => "integer".to_string(),
            Type::Object(_) => "object".to_string(),
            Type::Array(arr) => {
                if let Some(items) = &arr.items {
                    match items {
                        ReferenceOr::Reference { reference } => {
                            let name = reference.rsplit('/').next().unwrap_or(reference);
                            format!("array<[{name}]({name}.md)>")
                        }
                        ReferenceOr::Item(item) => {
                            format!("array<{}>", kind_to_string(&item.schema_kind))
                        }
                    }
                } else {
                    "array".to_string()
                }
            }
            Type::Boolean(_) => "boolean".to_string(),
        },
        SchemaKind::OneOf { one_of } => format!("oneOf({})", one_of.len()),
        SchemaKind::AnyOf { any_of } => format!("anyOf({})", any_of.len()),
        SchemaKind::AllOf { all_of } => format!("allOf({})", all_of.len()),
        SchemaKind::Not { .. } => "not".to_string(),
        SchemaKind::Any(_) => "any".to_string(),
    }
}

#[allow(dead_code)]
fn _schema_to_json(schema: &Schema) -> Value {
    serde_json::to_value(schema).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::parser::parse_spec;

    #[test]
    fn generates_type_markdown_from_schema() {
        let spec = r#"{
  "openapi": "3.0.0",
  "info": { "title": "T", "version": "v1" },
  "paths": {},
  "components": {
    "schemas": {
      "Thing": {
        "type": "object",
        "required": ["id"],
        "properties": {
          "id": { "type": "string", "description": "identifier" },
          "kind": { "type": "string", "enum": ["a", "b"] }
        }
      }
    }
  }
}"#;
        let spec_path =
            std::env::temp_dir().join(format!("apix-types-{}.json", std::process::id()));
        let out_root = std::env::temp_dir().join(format!("apix-types-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_root);
        std::fs::write(&spec_path, spec).expect("write");

        let parsed = parse_spec(spec_path.to_str().expect("path")).expect("parse");
        let n = generate_types(&parsed, &out_root, "demo").expect("generate");
        assert_eq!(n, 1);

        let rendered = std::fs::read_to_string(out_root.join("_types/Thing.md")).expect("read");
        assert!(rendered.contains("# Thing"));
        assert!(rendered.contains("Allowed values: a, b"));
        assert!(!rendered.contains("\n\n| `id`"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_dir_all(out_root);
    }
}
