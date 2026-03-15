use crate::error::ApixError;
use askama::Template;
use oas3::spec::{ObjectOrReference, ObjectSchema, ParameterIn};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

use super::parser::ParsedSpec;
use super::schema_helpers::{is_type, primary_type};
use oas3::spec::SchemaType;

#[derive(Debug)]
pub struct PropertyRow {
    pub name: String,
    pub required: String,
    pub prop_type: String,
    pub description: String,
}

#[derive(Template)]
#[template(path = "schema.md")]
struct SchemaTemplate<'a> {
    schema_type: &'a str,
    name: &'a str,
    description: &'a str,
    properties: &'a [PropertyRow],
}

#[derive(Template)]
#[template(path = "parameter.md")]
struct ParameterTemplate<'a> {
    name: &'a str,
    location: &'a str,
    required: bool,
    param_type: &'a str,
    description: &'a str,
}

#[derive(Template)]
#[template(path = "response.md")]
struct ResponseTemplate<'a> {
    name: &'a str,
    description: &'a str,
    // Add more fields if needed for better response docs
}

#[derive(Template)]
#[template(path = "generic_component.md")]
struct GenericTemplate<'a> {
    kind: &'a str,
    name: &'a str,
    content: String,
}

pub fn generate_components(
    parsed: &ParsedSpec,
    out_root: &Path,
    namespace: &str,
) -> Result<usize, ApixError> {
    let mut count = 0usize;
    let components_dir = out_root.join("_components");
    std::fs::create_dir_all(&components_dir)?;

    let Some(components) = &parsed.spec.components else {
        return Ok(0);
    };

    // 1. Schemas
    let schemas_dir = components_dir.join("schemas");
    std::fs::create_dir_all(&schemas_dir)?;
    for (name, schema_ref) in &components.schemas {
        let (schema_type, description, properties) = schema_details(schema_ref, namespace, parsed);
        let tpl = SchemaTemplate {
            schema_type: &schema_type,
            name,
            description: &description,
            properties: &properties,
        };
        let rendered = tpl.render().map_err(|err| {
            ApixError::Parse(format!("Failed to render schema template {name}: {err}"))
        })?;
        std::fs::write(schemas_dir.join(format!("{name}.md")), rendered)?;
        count += 1;
    }

    // 2. Parameters
    let params_dir = components_dir.join("parameters");
    std::fs::create_dir_all(&params_dir)?;
    for (name, param_ref) in &components.parameters {
        match param_ref {
            ObjectOrReference::Ref {
                ref_path: reference,
                ..
            } => {
                let tpl = GenericTemplate {
                    kind: "Parameter Reference",
                    name,
                    content: format!("Ref: {reference}"),
                };
                std::fs::write(params_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
            ObjectOrReference::Object(param) => {
                let location = match param.location {
                    ParameterIn::Query => "query",
                    ParameterIn::Path => "path",
                    ParameterIn::Header => "header",
                    ParameterIn::Cookie => "cookie",
                };
                let ptype = if let Some(schema_ref) = &param.schema {
                    match schema_ref {
                        ObjectOrReference::Ref {
                            ref_path: reference,
                            ..
                        } => {
                            let rname = reference.rsplit('/').next().unwrap_or(reference);
                            format!("[{rname}](../schemas/{rname}.md)")
                        }
                        ObjectOrReference::Object(inner) => {
                            schema_type_to_string(inner, "../schemas")
                        }
                    }
                } else {
                    "content".to_string()
                };
                let tpl = ParameterTemplate {
                    name,
                    location,
                    required: param.required.unwrap_or(false),
                    param_type: &ptype,
                    description: param.description.as_deref().unwrap_or_default(),
                };
                std::fs::write(params_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
        }
        count += 1;
    }

    // 3. Responses
    let responses_dir = components_dir.join("responses");
    std::fs::create_dir_all(&responses_dir)?;
    for (name, resp_ref) in &components.responses {
        match resp_ref {
            ObjectOrReference::Object(resp) => {
                let tpl = ResponseTemplate {
                    name,
                    description: resp.description.as_deref().unwrap_or_default(),
                };
                std::fs::write(
                    responses_dir.join(format!("{name}.md")),
                    tpl.render().unwrap(),
                )?;
            }
            ObjectOrReference::Ref {
                ref_path: reference,
                ..
            } => {
                let tpl = GenericTemplate {
                    kind: "Response Reference",
                    name,
                    content: format!("Ref: {reference}"),
                };
                std::fs::write(
                    responses_dir.join(format!("{name}.md")),
                    tpl.render().unwrap(),
                )?;
            }
        }
        count += 1;
    }

    // 4. Headers
    let headers_dir = components_dir.join("headers");
    std::fs::create_dir_all(&headers_dir)?;
    for (name, header_ref) in &components.headers {
        match header_ref {
            ObjectOrReference::Object(header) => {
                let tpl = GenericTemplate {
                    kind: "Header",
                    name,
                    content: header.description.clone().unwrap_or_default(),
                };
                std::fs::write(
                    headers_dir.join(format!("{name}.md")),
                    tpl.render().unwrap(),
                )?;
            }
            ObjectOrReference::Ref {
                ref_path: reference,
                ..
            } => {
                let tpl = GenericTemplate {
                    kind: "Header Reference",
                    name,
                    content: format!("Ref: {reference}"),
                };
                std::fs::write(
                    headers_dir.join(format!("{name}.md")),
                    tpl.render().unwrap(),
                )?;
            }
        }
        count += 1;
    }

    // 5. Request Bodies
    let bodies_dir = components_dir.join("requestBodies");
    std::fs::create_dir_all(&bodies_dir)?;
    for (name, body_ref) in &components.request_bodies {
        match body_ref {
            ObjectOrReference::Object(body) => {
                let tpl = GenericTemplate {
                    kind: "Request Body",
                    name,
                    content: format!(
                        "Description: {}",
                        body.description.as_deref().unwrap_or_default()
                    ),
                };
                std::fs::write(bodies_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
            ObjectOrReference::Ref {
                ref_path: reference,
                ..
            } => {
                let tpl = GenericTemplate {
                    kind: "Request Body Reference",
                    name,
                    content: format!("Ref: {reference}"),
                };
                std::fs::write(bodies_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
        }
        count += 1;
    }

    Ok(count)
}

fn schema_details(
    schema_ref: &ObjectOrReference<ObjectSchema>,
    namespace: &str,
    parsed: &ParsedSpec,
) -> (String, String, Vec<PropertyRow>) {
    match schema_ref {
        ObjectOrReference::Ref {
            ref_path: reference,
            ..
        } => (
            "reference".to_string(),
            format!("Reference to `{reference}`"),
            Vec::new(),
        ),
        ObjectOrReference::Object(schema) => {
            let schema_type = schema_type_to_string(schema, ".");
            let mut description = schema.description.clone().unwrap_or_default();
            if let Some(variants) = variant_links(schema, namespace, parsed) {
                if !description.is_empty() {
                    description.push_str("\n\n");
                }
                description.push_str(&variants);
            }
            let properties = collect_properties(schema, namespace, parsed);
            (schema_type, description, properties)
        }
    }
}

fn collect_properties(
    schema: &ObjectSchema,
    namespace: &str,
    parsed: &ParsedSpec,
) -> Vec<PropertyRow> {
    let mut rows = Vec::new();
    let required: HashSet<&str> = schema.required.iter().map(String::as_str).collect();

    // Properties from this schema
    for (prop_name, prop_schema) in &schema.properties {
        let (ptype, desc) = prop_type_and_description(prop_schema, namespace, parsed);
        rows.push(PropertyRow {
            name: super::sanitize_markdown_table_cell(prop_name),
            required: if required.contains(prop_name.as_str()) {
                "Yes".to_string()
            } else {
                "No".to_string()
            },
            prop_type: super::sanitize_markdown_table_cell(&ptype),
            description: super::sanitize_markdown_table_cell(&desc),
        });
    }

    // Propeties from all_of composition
    for item in &schema.all_of {
        match item {
            ObjectOrReference::Object(inner) => {
                rows.extend(collect_properties(inner, namespace, parsed));
            }
            ObjectOrReference::Ref {
                ref_path: reference,
                ..
            } => {
                let mut seen = HashSet::new();
                if let Ok(resolved) =
                    crate::build::resolver::resolve_schema(reference, &parsed.spec, &mut seen)
                {
                    rows.extend(collect_properties(resolved, namespace, parsed));
                } else {
                    let name = reference.rsplit('/').next().unwrap_or(reference);
                    rows.push(PropertyRow {
                        name: super::sanitize_markdown_table_cell(&format!("(ref: {name})")),
                        required: "Unknown".to_string(),
                        prop_type: super::sanitize_markdown_table_cell(&format!(
                            "[{name}]({name}.md)"
                        )),
                        description: super::sanitize_markdown_table_cell("Unresolved reference"),
                    });
                }
            }
        }
    }
    rows
}

fn prop_type_and_description(
    prop_schema: &ObjectOrReference<ObjectSchema>,
    _namespace: &str,
    _parsed: &ParsedSpec,
) -> (String, String) {
    match prop_schema {
        ObjectOrReference::Ref {
            ref_path: reference,
            ..
        } => {
            let name = reference.rsplit('/').next().unwrap_or(reference);
            (format!("[{name}]({name}.md)"), String::new())
        }
        ObjectOrReference::Object(inner) => {
            let ptype = schema_type_to_string(inner, ".");
            let mut description = inner.description.clone().unwrap_or_default();
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

fn string_enum_values(schema: &ObjectSchema) -> Option<Vec<String>> {
    let vals: Vec<String> = schema
        .enum_values
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    if vals.is_empty() { None } else { Some(vals) }
}

fn variant_links(schema: &ObjectSchema, _namespace: &str, _parsed: &ParsedSpec) -> Option<String> {
    let mut variants = Vec::new();
    variants.extend(&schema.one_of);
    variants.extend(&schema.any_of);

    let refs: Vec<String> = variants
        .iter()
        .map(|item| match item {
            ObjectOrReference::Ref {
                ref_path: reference,
                ..
            } => {
                let name = reference.rsplit('/').next().unwrap_or(reference);
                format!("- [{name}]({name}.md)")
            }
            ObjectOrReference::Object(_) => "- (Inline Schema)".to_string(),
        })
        .collect();

    if refs.is_empty() {
        None
    } else {
        Some(format!("Variants:\n{}", refs.join("\n")))
    }
}

pub(crate) fn schema_type_to_string(schema: &ObjectSchema, schema_dir_rel: &str) -> String {
    // Check composition first
    if !schema.one_of.is_empty() {
        return format!("oneOf({})", schema.one_of.len());
    }
    if !schema.any_of.is_empty() {
        return format!("anyOf({})", schema.any_of.len());
    }
    if !schema.all_of.is_empty() {
        return format!("allOf({})", schema.all_of.len());
    }

    // Check array with items
    if is_type(schema, SchemaType::Array) {
        if let Some(items) = &schema.items {
            return match items.as_ref() {
                oas3::spec::Schema::Object(box_ref) => match box_ref.as_ref() {
                    ObjectOrReference::Ref {
                        ref_path: reference,
                        ..
                    } => {
                        let name = reference.rsplit('/').next().unwrap_or(reference);
                        format!("array<[{name}]({schema_dir_rel}/{name}.md)>")
                    }
                    ObjectOrReference::Object(inner) => {
                        format!("array<{}>", schema_type_to_string(inner, schema_dir_rel))
                    }
                },
                _ => "array".to_string(),
            };
        }
        return "array".to_string();
    }

    match primary_type(schema) {
        Some(SchemaType::String) => "string",
        Some(SchemaType::Number) => "number",
        Some(SchemaType::Integer) => "integer",
        Some(SchemaType::Object) => "object",
        Some(SchemaType::Array) => "array",
        Some(SchemaType::Boolean) => "boolean",
        Some(SchemaType::Null) => "null",
        None => "any",
    }
    .to_string()
}

#[allow(dead_code)]
fn _schema_to_json(schema: &ObjectSchema) -> Value {
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
            std::env::temp_dir().join(format!("apix-components-{}.json", std::process::id()));
        let out_root =
            std::env::temp_dir().join(format!("apix-components-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_root);
        std::fs::write(&spec_path, spec).expect("write");

        let parsed = parse_spec(spec_path.to_str().expect("path")).expect("parse");
        let n = generate_components(&parsed, &out_root, "demo").expect("generate");
        assert_eq!(n, 1);

        let rendered =
            std::fs::read_to_string(out_root.join("_components/schemas/Thing.md")).expect("read");
        assert!(rendered.contains("# Thing"));
        assert!(rendered.contains("Allowed values: a, b"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_dir_all(out_root);
    }

    #[test]
    fn escapes_multiline_schema_property_description_in_table_cell() {
        let spec = r#"{
  "openapi": "3.0.0",
  "info": { "title": "T", "version": "v1" },
  "paths": {},
  "components": {
    "schemas": {
      "Thing": {
        "type": "object",
        "properties": {
          "notes": { "type": "string", "description": "line one\nline two" }
        }
      }
    }
  }
}"#;
        let spec_path = std::env::temp_dir().join(format!(
            "apix-components-multiline-{}.json",
            std::process::id()
        ));
        let out_root = std::env::temp_dir().join(format!(
            "apix-components-multiline-out-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&out_root);
        std::fs::write(&spec_path, spec).expect("write");

        let parsed = parse_spec(spec_path.to_str().expect("path")).expect("parse");
        let _ = generate_components(&parsed, &out_root, "demo").expect("generate");

        let rendered =
            std::fs::read_to_string(out_root.join("_components/schemas/Thing.md")).expect("read");
        assert!(rendered.contains("line one<br/>line two"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_dir_all(out_root);
    }
}
