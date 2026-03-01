use crate::error::ApixError;
use askama::Template;
use openapiv3::{
    MediaType, Operation, Parameter, ParameterSchemaOrContent, PathItem, ReferenceOr, Response,
    Schema, SchemaKind, StatusCode, Type,
};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::parser::ParsedSpec;

#[derive(Debug)]
struct ParamRow {
    name: String,
    required: String,
    param_type: String,
    description: String,
}

#[derive(Debug)]
struct ResponseRow {
    status: String,
    description: String,
}

#[derive(Template)]
#[template(path = "route.md")]
struct RouteTemplate<'a> {
    method: &'a str,
    url: &'a str,
    auth: &'a str,
    content_type: &'a str,
    summary: &'a str,
    description: &'a str,
    path_params: &'a [ParamRow],
    query_params: &'a [ParamRow],
    request_body: &'a str,
    responses: &'a [ResponseRow],
}

pub fn generate_routes(
    parsed: &ParsedSpec,
    out_root: &Path,
    namespace: &str,
) -> Result<usize, ApixError> {
    let mut count = 0usize;
    let mut created_dirs: HashSet<PathBuf> = HashSet::new();

    for (path, path_item_ref) in &parsed.openapi.paths.paths {
        let ReferenceOr::Item(path_item) = path_item_ref else {
            continue;
        };

        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            path_item,
            "GET",
            path_item.get.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            path_item,
            "POST",
            path_item.post.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            path_item,
            "PUT",
            path_item.put.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            path_item,
            "PATCH",
            path_item.patch.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            path_item,
            "DELETE",
            path_item.delete.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            path_item,
            "HEAD",
            path_item.head.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            path_item,
            "OPTIONS",
            path_item.options.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            path_item,
            "TRACE",
            path_item.trace.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
    }

    Ok(count)
}

fn emit_operation(
    parsed: &ParsedSpec,
    out_root: &Path,
    namespace: &str,
    path: &str,
    path_item: &PathItem,
    method: &str,
    operation: Option<&Operation>,
    count: &mut usize,
    created_dirs: &mut HashSet<PathBuf>,
) -> Result<(), ApixError> {
    let Some(op) = operation else {
        return Ok(());
    };

    let (path_params, query_params) = collect_parameters(path_item, op);
    let content_type = request_content_type(op).unwrap_or_else(|| "application/json".to_string());
    let request_body = request_body_text(op, namespace, &parsed.version);
    let responses = response_rows(op);

    let summary = op.summary.as_deref().unwrap_or(method);
    let description = op.description.as_deref().unwrap_or_default();
    let url = format!("{}{}", parsed.base_url, path);

    let tpl = RouteTemplate {
        method,
        url: &url,
        auth: "Unknown",
        content_type: &content_type,
        summary,
        description,
        path_params: &path_params,
        query_params: &query_params,
        request_body: &request_body,
        responses: &responses,
    };

    let rendered = tpl.render().map_err(|err| {
        ApixError::Parse(format!(
            "Failed to render route template {method} {path}: {err}"
        ))
    })?;

    let out_dir = route_dir(out_root, path);
    if created_dirs.insert(out_dir.clone()) {
        std::fs::create_dir_all(&out_dir)?;
    }
    std::fs::write(out_dir.join(format!("{method}.md")), rendered)?;
    *count += 1;
    Ok(())
}

fn route_dir(root: &Path, path: &str) -> PathBuf {
    let mut out = root.to_path_buf();
    for segment in path.trim_start_matches('/').split('/') {
        if !segment.is_empty() {
            out.push(segment);
        }
    }
    out
}

fn collect_parameters(path_item: &PathItem, op: &Operation) -> (Vec<ParamRow>, Vec<ParamRow>) {
    let mut path_rows = Vec::new();
    let mut query_rows = Vec::new();

    let mut all_params = Vec::new();
    all_params.extend(path_item.parameters.iter());
    all_params.extend(op.parameters.iter());

    for param_ref in all_params {
        match param_ref {
            ReferenceOr::Reference { reference } => {
                let name = reference
                    .rsplit('/')
                    .next()
                    .unwrap_or(reference)
                    .to_string();
                query_rows.push(ParamRow {
                    name,
                    required: "Unknown".to_string(),
                    param_type: "ref".to_string(),
                    description: format!("Reference: {reference}"),
                });
            }
            ReferenceOr::Item(param) => match param {
                Parameter::Query { parameter_data, .. } => {
                    query_rows.push(row_from_parameter_data(parameter_data));
                }
                Parameter::Path { parameter_data, .. } => {
                    path_rows.push(row_from_parameter_data(parameter_data));
                }
                Parameter::Header { parameter_data, .. }
                | Parameter::Cookie { parameter_data, .. } => {
                    query_rows.push(row_from_parameter_data(parameter_data));
                }
            },
        }
    }

    (path_rows, query_rows)
}

fn row_from_parameter_data(data: &openapiv3::ParameterData) -> ParamRow {
    let ty = match &data.format {
        ParameterSchemaOrContent::Schema(ref_or_schema) => match ref_or_schema {
            ReferenceOr::Reference { reference } => {
                let name = reference.rsplit('/').next().unwrap_or(reference);
                format!("ref:{name}")
            }
            ReferenceOr::Item(schema) => super::types::kind_to_string(&schema.schema_kind),
        },
        ParameterSchemaOrContent::Content(_) => "content".to_string(),
    };

    ParamRow {
        name: data.name.clone(),
        required: if data.required { "Yes" } else { "No" }.to_string(),
        param_type: ty,
        description: data.description.clone().unwrap_or_default(),
    }
}

fn request_content_type(op: &Operation) -> Option<String> {
    let request_body = op.request_body.as_ref()?;
    match request_body {
        ReferenceOr::Reference { .. } => Some("application/json".to_string()),
        ReferenceOr::Item(item) => item.content.keys().next().cloned(),
    }
}

fn request_body_text(op: &Operation, namespace: &str, version: &str) -> String {
    match &op.request_body {
        None => String::new(),
        Some(ReferenceOr::Reference { reference }) => {
            let name = reference.rsplit('/').next().unwrap_or(reference);
            format!("`apix peek {namespace}/{version}/_types/{name}`")
        }
        Some(ReferenceOr::Item(item)) => {
            if item.content.is_empty() {
                return "Request body present but no media type entries".to_string();
            }
            let mut out = String::new();
            out.push_str("Supported content types:\n");
            for ctype in item.content.keys() {
                out.push_str(&format!("- `{ctype}`\n"));
            }
            if let Some((ctype, media_type)) = item.content.iter().next() {
                out.push('\n');
                out.push_str(&inline_body_doc(ctype, media_type, namespace, version));
            }
            out.trim_end().to_string()
        }
    }
}

fn inline_body_doc(ctype: &str, media_type: &MediaType, namespace: &str, version: &str) -> String {
    let Some(schema_ref) = &media_type.schema else {
        return format!("No schema provided for `{ctype}`.");
    };

    let mut out = String::new();
    out.push_str(&format!("### Inline Request Schema (`{ctype}`)\n"));
    match schema_ref {
        ReferenceOr::Reference { reference } => {
            let name = reference.rsplit('/').next().unwrap_or(reference);
            out.push_str(&format!(
                "`apix peek {namespace}/{version}/_types/{name}`\n"
            ));
        }
        ReferenceOr::Item(schema) => {
            let rows = schema_property_rows(schema);
            if !rows.is_empty() {
                out.push_str("| Property | Required | Type | Description |\n");
                out.push_str("| :--- | :---: | :--- | :--- |\n");
                for (name, required, ty, desc) in rows {
                    out.push_str(&format!(
                        "| `{}` | {} | {} | {} |\n",
                        name,
                        if required { "Yes" } else { "No" },
                        ty,
                        desc
                    ));
                }
            } else {
                out.push_str("*(No object properties found)*\n");
            }

            out.push('\n');
            out.push_str("### Example Payload\n");
            out.push_str("```json\n");
            let ex = schema_example_json(schema);
            let rendered = serde_json::to_string_pretty(&ex).unwrap_or_else(|_| "{}".to_string());
            out.push_str(&rendered);
            out.push_str("\n```\n");
        }
    }

    out
}

fn schema_property_rows(schema: &Schema) -> Vec<(String, bool, String, String)> {
    match &schema.schema_kind {
        SchemaKind::Type(Type::Object(obj)) => obj
            .properties
            .iter()
            .map(|(name, prop)| {
                let (ty, desc) = match prop {
                    ReferenceOr::Reference { reference } => {
                        let p = reference.rsplit('/').next().unwrap_or(reference);
                        (format!("ref:{p}"), format!("Reference: {reference}"))
                    }
                    ReferenceOr::Item(inner) => (
                        super::types::kind_to_string(&inner.schema_kind),
                        inner.schema_data.description.clone().unwrap_or_default(),
                    ),
                };
                (
                    name.clone(),
                    obj.required.iter().any(|r| r == name),
                    ty,
                    desc,
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn schema_example_json(schema: &Schema) -> Value {
    match &schema.schema_kind {
        SchemaKind::Type(Type::Object(obj)) => {
            let mut map = Map::new();
            for (name, prop) in &obj.properties {
                let value = match prop {
                    ReferenceOr::Reference { .. } => Value::String("ref-value".to_string()),
                    ReferenceOr::Item(inner) => schema_example_json(inner),
                };
                map.insert(name.clone(), value);
            }
            Value::Object(map)
        }
        SchemaKind::Type(Type::Array(arr)) => {
            let item = arr
                .items
                .as_ref()
                .map(|i| match i {
                    ReferenceOr::Reference { .. } => Value::String("ref-item".to_string()),
                    ReferenceOr::Item(schema) => schema_example_json(schema),
                })
                .unwrap_or(Value::Null);
            Value::Array(vec![item])
        }
        SchemaKind::Type(Type::String(st)) => {
            if let Some(Some(first)) = st.enumeration.first() {
                Value::String(first.clone())
            } else {
                Value::String("string".to_string())
            }
        }
        SchemaKind::Type(Type::Integer(_)) => json!(0),
        SchemaKind::Type(Type::Number(_)) => json!(0.0),
        SchemaKind::Type(Type::Boolean(_)) => json!(true),
        _ => Value::Null,
    }
}

fn response_rows(op: &Operation) -> Vec<ResponseRow> {
    let mut rows = Vec::new();
    for (status, response_ref) in &op.responses.responses {
        let status_text = match status {
            StatusCode::Code(code) => code.to_string(),
            StatusCode::Range(range) => format!("{range:?}xx"),
        };
        rows.push(ResponseRow {
            status: status_text,
            description: response_description(response_ref),
        });
    }
    if let Some(default) = &op.responses.default {
        rows.push(ResponseRow {
            status: "default".to_string(),
            description: response_description(default),
        });
    }
    rows
}

fn response_description(response_ref: &ReferenceOr<Response>) -> String {
    match response_ref {
        ReferenceOr::Reference { reference } => format!("Reference: {reference}"),
        ReferenceOr::Item(item) => item.description.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::parser::parse_spec;

    #[test]
    fn generates_route_markdown_with_params() {
        let spec = r#"{
  "openapi": "3.0.0",
  "info": { "title": "T", "version": "v1" },
  "servers": [{ "url": "https://api.example.com" }],
  "paths": {
    "/items/{id}": {
      "post": {
        "summary": "Create",
        "parameters": [
          { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } },
          { "name": "verbose", "in": "query", "required": false, "schema": { "type": "boolean" } }
        ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "required": ["name"],
                "properties": {
                  "name": { "type": "string" },
                  "count": { "type": "integer" }
                }
              }
            }
          }
        },
        "responses": { "201": { "description": "Created" } }
      }
    }
  }
}"#;
        let spec_path =
            std::env::temp_dir().join(format!("apix-routes-{}.json", std::process::id()));
        let out_root = std::env::temp_dir().join(format!("apix-routes-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_root);
        std::fs::write(&spec_path, spec).expect("write");

        let parsed = parse_spec(spec_path.to_str().expect("path")).expect("parse");
        let n = generate_routes(&parsed, &out_root, "demo").expect("generate");
        assert_eq!(n, 1);

        let rendered =
            std::fs::read_to_string(out_root.join("items/{id}/POST.md")).expect("read route");
        assert!(rendered.contains("## Path Parameters"));
        assert!(rendered.contains("## Query Parameters"));
        assert!(rendered.contains("## Request Body"));
        assert!(rendered.contains("### Example Payload"));
        assert!(rendered.contains("**201**"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_dir_all(out_root);
    }

    #[test]
    fn request_body_ref_commands_include_namespace_and_version() {
        let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "T", "version": "v9" },
  "servers": [{ "url": "https://api.example.com" }],
  "paths": {
    "/items": {
      "post": {
        "summary": "Create",
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": { "$ref": "#/components/schemas/ItemCreate" }
            }
          }
        },
        "responses": { "201": { "description": "Created" } }
      }
    }
  },
  "components": {
    "schemas": {
      "ItemCreate": {
        "type": "object",
        "properties": {
          "name": { "type": "string" }
        }
      }
    }
  }
}"##;
        let spec_path =
            std::env::temp_dir().join(format!("apix-routes-ref-{}.json", std::process::id()));
        let out_root =
            std::env::temp_dir().join(format!("apix-routes-ref-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_root);
        std::fs::write(&spec_path, spec).expect("write");

        let parsed = parse_spec(spec_path.to_str().expect("path")).expect("parse");
        let _ = generate_routes(&parsed, &out_root, "demo").expect("generate");
        let rendered = std::fs::read_to_string(out_root.join("items/POST.md")).expect("read");
        assert!(rendered.contains("apix peek demo/v9/_types/ItemCreate"));
        assert!(!rendered.contains("apix peek _types/ItemCreate"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_dir_all(out_root);
    }
}
