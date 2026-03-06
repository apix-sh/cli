use crate::error::ApixError;
use askama::Template;
use openapiv3::{ReferenceOr, Schema, SchemaKind, Type, Parameter};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

use super::parser::ParsedSpec;

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

    let Some(components) = &parsed.openapi.components else {
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
            ReferenceOr::Reference { reference } => {
                let tpl = GenericTemplate { kind: "Parameter Reference", name, content: format!("Ref: {reference}") };
                std::fs::write(params_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
            ReferenceOr::Item(param) => {
                let (location, parameter_data) = match param {
                    Parameter::Query { parameter_data, .. } => ("query", parameter_data),
                    Parameter::Path { parameter_data, .. } => ("path", parameter_data),
                    Parameter::Header { parameter_data, .. } => ("header", parameter_data),
                    Parameter::Cookie { parameter_data, .. } => ("cookie", parameter_data),
                };
                let ptype = match &parameter_data.format {
                    openapiv3::ParameterSchemaOrContent::Schema(s) => match s {
                        ReferenceOr::Reference { reference } => {
                            let rname = reference.rsplit('/').next().unwrap_or(reference);
                            format!("[{rname}](../schemas/{rname}.md)")
                        }
                        ReferenceOr::Item(inner) => kind_to_string(&inner.schema_kind, "../schemas"),
                    },
                    _ => "content".to_string(),
                };
                let tpl = ParameterTemplate {
                    name,
                    location,
                    required: parameter_data.required,
                    param_type: &ptype,
                    description: parameter_data.description.as_deref().unwrap_or_default(),
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
            ReferenceOr::Item(resp) => {
                let tpl = ResponseTemplate { name, description: &resp.description };
                std::fs::write(responses_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
            ReferenceOr::Reference { reference } => {
                let tpl = GenericTemplate { kind: "Response Reference", name, content: format!("Ref: {reference}") };
                std::fs::write(responses_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
        }
        count += 1;
    }

    // 4. Headers
    let headers_dir = components_dir.join("headers");
    std::fs::create_dir_all(&headers_dir)?;
    for (name, header_ref) in &components.headers {
        match header_ref {
             ReferenceOr::Item(header) => {
                let tpl = GenericTemplate { kind: "Header", name, content: header.description.clone().unwrap_or_default() };
                std::fs::write(headers_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
            ReferenceOr::Reference { reference } => {
                let tpl = GenericTemplate { kind: "Header Reference", name, content: format!("Ref: {reference}") };
                std::fs::write(headers_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
        }
        count += 1;
    }

    // 5. Request Bodies
    let bodies_dir = components_dir.join("requestBodies");
    std::fs::create_dir_all(&bodies_dir)?;
    for (name, body_ref) in &components.request_bodies {
        match body_ref {
            ReferenceOr::Item(body) => {
                let tpl = GenericTemplate { kind: "Request Body", name, content: format!("Description: {}", body.description.as_deref().unwrap_or_default()) };
                std::fs::write(bodies_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
            ReferenceOr::Reference { reference } => {
                let tpl = GenericTemplate { kind: "Request Body Reference", name, content: format!("Ref: {reference}") };
                std::fs::write(bodies_dir.join(format!("{name}.md")), tpl.render().unwrap())?;
            }
        }
        count += 1;
    }

    Ok(count)
}

fn schema_details(
    schema_ref: &ReferenceOr<Schema>,
    namespace: &str,
    parsed: &ParsedSpec,
) -> (String, String, Vec<PropertyRow>) {
    match schema_ref {
        ReferenceOr::Reference { reference } => (
            "reference".to_string(),
            format!("Reference to `{reference}`"),
            Vec::new(),
        ),
        ReferenceOr::Item(schema) => {
            let schema_type = kind_to_string(&schema.schema_kind, ".");
            let mut description = schema.schema_data.description.clone().unwrap_or_default();
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

fn collect_properties(schema: &Schema, namespace: &str, parsed: &ParsedSpec) -> Vec<PropertyRow> {
    match &schema.schema_kind {
        SchemaKind::Type(Type::Object(obj)) => {
            let mut rows = Vec::new();
            let required: HashSet<&str> = obj.required.iter().map(String::as_str).collect();
            for (prop_name, prop_schema) in &obj.properties {
                let (ptype, desc) = prop_type_and_description(prop_schema, namespace, parsed);
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
                match item {
                    ReferenceOr::Item(inner) => {
                        rows.extend(collect_properties(inner, namespace, parsed));
                    }
                    ReferenceOr::Reference { reference } => {
                        let mut seen = HashSet::new();
                        if let Ok(resolved) = crate::build::resolver::resolve_schema(
                            reference,
                            &parsed.openapi,
                            &mut seen,
                        ) {
                            rows.extend(collect_properties(resolved, namespace, parsed));
                        } else {
                            let name = reference.rsplit('/').next().unwrap_or(reference);
                            rows.push(PropertyRow {
                                name: format!("(ref: {name})"),
                                required: "Unknown".to_string(),
                                prop_type: format!("[{name}]({name}.md)"),
                                description: "Unresolved reference".to_string(),
                            });
                        }
                    }
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
    _parsed: &ParsedSpec,
) -> (String, String) {
    match prop_schema {
        ReferenceOr::Reference { reference } => {
            let name = reference.rsplit('/').next().unwrap_or(reference);
            (format!("[{name}]({name}.md)"), String::new())
        }
        ReferenceOr::Item(inner) => {
            let ptype = kind_to_string(&inner.schema_kind, ".");
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

fn variant_links(schema: &Schema, _namespace: &str, _parsed: &ParsedSpec) -> Option<String> {
    let variants: Vec<&ReferenceOr<Schema>> = match &schema.schema_kind {
        SchemaKind::OneOf { one_of } => one_of.iter().collect(),
        SchemaKind::AnyOf { any_of } => any_of.iter().collect(),
        _ => return None,
    };

    let refs: Vec<String> = variants
        .iter()
        .map(|item| match item {
            ReferenceOr::Reference { reference } => {
                let name = reference.rsplit('/').next().unwrap_or(reference);
                format!("- [{name}]({name}.md)")
            }
            ReferenceOr::Item(_) => "- (Inline Schema)".to_string(),
        })
        .collect();

    if refs.is_empty() {
        None
    } else {
        Some(format!("Variants:\n{}", refs.join("\n")))
    }
}

pub(crate) fn kind_to_string(kind: &SchemaKind, schema_dir_rel: &str) -> String {
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
                            format!("array<[{name}]({schema_dir_rel}/{name}.md)>")
                        }
                        ReferenceOr::Item(item) => {
                            format!("array<{}>", kind_to_string(&item.schema_kind, schema_dir_rel))
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
            std::env::temp_dir().join(format!("apix-components-{}.json", std::process::id()));
        let out_root = std::env::temp_dir().join(format!("apix-components-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_root);
        std::fs::write(&spec_path, spec).expect("write");

        let parsed = parse_spec(spec_path.to_str().expect("path")).expect("parse");
        let n = generate_components(&parsed, &out_root, "demo").expect("generate");
        assert_eq!(n, 1);

        let rendered = std::fs::read_to_string(out_root.join("_components/schemas/Thing.md")).expect("read");
        assert!(rendered.contains("# Thing"));
        assert!(rendered.contains("Allowed values: a, b"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_dir_all(out_root);
    }
}
