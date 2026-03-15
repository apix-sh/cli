use crate::error::ApixError;
use askama::Template;
use oas3::spec::{
    MediaType, ObjectOrReference, ObjectSchema, Operation, Parameter, ParameterIn, PathItem, Response,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
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
    headers: Vec<ParamRow>,
    content: String,
}

#[derive(Template)]
#[template(path = "route.md")]
struct RouteTemplate<'a> {
    method: &'a str,
    url: &'a str,
    auth: Option<&'a str>,
    content_type: &'a str,
    summary: &'a str,
    description: &'a str,
    path_params: &'a [ParamRow],
    query_params: &'a [ParamRow],
    header_params: &'a [ParamRow],
    cookie_params: &'a [ParamRow],
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

    let empty_map = BTreeMap::new();
    let paths = parsed.spec.paths.as_ref().unwrap_or(&empty_map);
    for (path, path_item) in paths {
        // PathItem in oas3 is direct, not wrapped in ReferenceOr in the map usually
        // but wait, oas3::Spec.paths is Option<BTreeMap<String, PathItem>>
        // and PathItem DOES NOT have a ReferenceOr variant in that map.
        // It has a Reference field if it's a ref.
        
        // If it's a reference, we need to resolve it.
        let resolved_path_item = if let Some(reference) = &path_item.reference {
             let mut seen = std::collections::HashSet::new();
             crate::build::resolver::resolve_path_item(reference, &parsed.spec, &mut seen)?
        } else {
             path_item
        };

        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            resolved_path_item,
            "GET",
            resolved_path_item.get.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            resolved_path_item,
            "POST",
            resolved_path_item.post.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            resolved_path_item,
            "PUT",
            resolved_path_item.put.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            resolved_path_item,
            "PATCH",
            resolved_path_item.patch.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            resolved_path_item,
            "DELETE",
            resolved_path_item.delete.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            resolved_path_item,
            "HEAD",
            resolved_path_item.head.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            resolved_path_item,
            "OPTIONS",
            resolved_path_item.options.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
        emit_operation(
            parsed,
            out_root,
            namespace,
            path,
            resolved_path_item,
            "TRACE",
            resolved_path_item.trace.as_ref(),
            &mut count,
            &mut created_dirs,
        )?;
    }

    Ok(count)
}

#[allow(clippy::too_many_arguments)]
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

    let depth = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .count();
    let base_components_rel = if depth == 0 {
        "_components".to_string()
    } else {
        format!("{}_components", "../".repeat(depth))
    };

    let (path_params, query_params, header_params, cookie_params) =
        collect_parameters(path_item, op, &base_components_rel);
    let content_type = request_content_type(op).unwrap_or_else(|| "application/json".to_string());
    let request_body = request_body_text(op, namespace, &parsed.version, &base_components_rel);
    let responses = response_rows(op, namespace, &parsed.version, &base_components_rel);

    let summary = op.summary.as_deref().unwrap_or(method);
    let description = op.description.as_deref().unwrap_or_default();
    let url = format!("{}{}", parsed.base_url, path);

    let op_auth = super::parser::format_security_schemes(&op.security, &parsed.spec.components);
    let auth_string = if op_auth == parsed.auth {
        None
    } else {
        Some(op_auth)
    };


    let tpl = RouteTemplate {
        method,
        url: &url,
        auth: auth_string.as_deref(),
        content_type: &content_type,
        summary,
        description,
        path_params: &path_params,
        query_params: &query_params,
        header_params: &header_params,
        cookie_params: &cookie_params,
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

fn collect_parameters(
    path_item: &PathItem,
    op: &Operation,
    base_components_rel: &str,
) -> (Vec<ParamRow>, Vec<ParamRow>, Vec<ParamRow>, Vec<ParamRow>) {
    let mut path_rows = Vec::new();
    let mut query_rows = Vec::new();
    let mut header_rows = Vec::new();
    let mut cookie_rows = Vec::new();

    let mut all_params = Vec::new();
    all_params.extend(path_item.parameters.iter());
    all_params.extend(op.parameters.iter());

    for param_ref in all_params {
        match param_ref {
            ObjectOrReference::Ref { ref_path: reference, .. } => {
                let link = ref_to_link(reference, base_components_rel);
                query_rows.push(ParamRow {
                    name: "Reference".to_string(),
                    required: "N/A".to_string(),
                    param_type: link,
                    description: String::new(),
                });
            }
            ObjectOrReference::Object(param) => {
                let bucket = match param.location {
                    ParameterIn::Path => &mut path_rows,
                    ParameterIn::Query => &mut query_rows,
                    ParameterIn::Header => &mut header_rows,
                    ParameterIn::Cookie => &mut cookie_rows,
                };
                bucket.push(row_from_parameter(param, base_components_rel));
            }
        }
    }

    (path_rows, query_rows, header_rows, cookie_rows)
}

fn row_from_parameter(
    param: &Parameter,
    base_components_rel: &str,
) -> ParamRow {
    let ty = if let Some(schema_ref) = &param.schema {
        match schema_ref {
            ObjectOrReference::Ref { ref_path: reference, .. } => {
                ref_to_link(reference, base_components_rel)
            }
            ObjectOrReference::Object(schema) => {
                super::components::schema_type_to_string(schema, &format!("{}/schemas", base_components_rel))
            }
        }
    } else if let Some(content) = &param.content {
        if let Some((ctype, _media_type)) = content.iter().next() {
            format!("content `{ctype}`")
        } else {
            "content".to_string()
        }
    } else {
        "any".to_string()
    };

    let mut hints = Vec::new();
    if let Some(s) = &param.style {
        hints.push(format!("style={s:?}"));
    }
    if let Some(explode) = param.explode {
        hints.push(format!("explode={explode}"));
    }
    if let Some(allow) = param.allow_reserved
        && allow {
            hints.push("allowReserved=true".to_string());
        }

    let mut desc = param.description.clone().unwrap_or_default();
    if !hints.is_empty() {
        let hint_str = format!("*Serialization: {}*", hints.join(", "));
        if desc.is_empty() {
            desc = hint_str;
        } else {
            desc.push_str(&format!("<br/>{}", hint_str));
        }
    }

    ParamRow {
        name: param.name.clone(),
        required: if param.required.unwrap_or(false) { "Yes" } else { "No" }.to_string(),
        param_type: ty,
        description: desc,
    }
}

fn request_content_type(op: &Operation) -> Option<String> {
    let request_body = op.request_body.as_ref()?;
    match request_body {
        ObjectOrReference::Ref { .. } => Some("application/json".to_string()),
        ObjectOrReference::Object(item) => item.content.keys().next().cloned(),
    }
}

fn request_body_text(op: &Operation, namespace: &str, version: &str, base_components_rel: &str) -> String {
    match &op.request_body {
        None => String::new(),
        Some(ObjectOrReference::Ref { ref_path: reference, .. }) => {
            ref_to_link(reference, base_components_rel)
        }
        Some(ObjectOrReference::Object(item)) => {
            if item.content.is_empty() {
                return "Request body present but no media type entries".to_string();
            }
            let mut out = String::new();
            out.push_str("Supported content types:\n");
            for ctype in item.content.keys() {
                out.push_str(&format!("- `{ctype}`\n"));
            }

            for (ctype, media_type) in item.content.iter() {
                out.push('\n');
                out.push_str(&inline_body_doc(
                    "### Inline Request Schema",
                    ctype,
                    media_type,
                    namespace,
                    version,
                    base_components_rel,
                ));
            }
            out.trim_end().to_string()
        }
    }
}

fn inline_body_doc(
    title_prefix: &str,
    ctype: &str,
    media_type: &MediaType,
    _namespace: &str,
    _version: &str,
    base_components_rel: &str,
) -> String {
    let Some(schema_ref) = &media_type.schema else {
        return format!("No schema provided for `{ctype}`.");
    };

    let mut out = String::new();
    out.push_str(&format!("{title_prefix} (`{ctype}`)\n"));
    
    match schema_ref {
        ObjectOrReference::Ref { ref_path: reference, .. } => {
            out.push_str(&format!("{}\n", ref_to_link(reference, base_components_rel)));
        }
        ObjectOrReference::Object(schema) => {
            let rows = schema_property_rows(schema, base_components_rel);
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
                let kind_str = super::components::schema_type_to_string(schema, &format!("{}/schemas", base_components_rel));
                if kind_str.starts_with("array<") {
                    out.push_str(&format!("{kind_str}\n"));
                } else {
                    out.push_str("*(No object properties found)*\n");
                }
            }

            // In oas3 0.20, MediaType example is under examples or deprecated?
            // Let's check common alternatives if .example failed.
            // Actually, I'll just use schema example for now to unblock.
            let ex = schema_example_json(schema);
            if let Some(ex_val) = ex {
                out.push('\n');
                out.push_str("#### Example Payload\n");
                if ctype == "application/x-www-form-urlencoded" {
                    out.push_str("```text\n");
                    out.push_str(&url_encoded_example(&ex_val));
                    out.push_str("\n```\n");
                } else if ctype == "multipart/form-data" {
                    out.push_str("```text\n");
                    out.push_str(&multipart_example(&ex_val));
                    out.push_str("\n```\n");
                } else {
                    out.push_str("```json\n");
                    let rendered =
                        serde_json::to_string_pretty(&ex_val).unwrap_or_else(|_| "{}".to_string());
                    out.push_str(&rendered);
                    out.push_str("\n```\n");
                }
            }
        }
    }

    out
}

fn schema_property_rows(
    schema: &ObjectSchema,
    base_components_rel: &str,
) -> Vec<(String, bool, String, String)> {
    let mut rows = Vec::new();
    let required: HashSet<&str> = schema.required.iter().map(String::as_str).collect();

    for (name, prop_ref) in &schema.properties {
        let (ty, desc) = match prop_ref {
            ObjectOrReference::Ref { ref_path: reference, .. } => {
                (ref_to_link(reference, base_components_rel), String::new())
            }
            ObjectOrReference::Object(inner) => (
                super::components::schema_type_to_string(inner, &format!("{}/schemas", base_components_rel)),
                inner.description.clone().unwrap_or_default(),
            ),
        };
        rows.push((
            name.clone(),
            required.contains(name.as_str()),
            ty,
            desc,
        ));
    }
    rows
}

fn schema_example_json(schema: &ObjectSchema) -> Option<Value> {
    if schema.example.is_some() {
        return schema.example.clone();
    }
    if schema.default.is_some() {
        return schema.default.clone();
    }
    None
}

fn url_encoded_example(val: &Value) -> String {
    let mut pairs = Vec::new();
    if let Value::Object(map) = val {
        for (k, v) in map {
            let val_str = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            pairs.push(format!("{}={}", k, val_str));
        }
    }
    pairs.join("&")
}

fn multipart_example(val: &Value) -> String {
    let mut out = String::new();
    if let Value::Object(map) = val {
        for (k, v) in map {
            out.push_str("--boundary\n");
            out.push_str(&format!(
                "Content-Disposition: form-data; name=\"{}\"\n\n",
                k
            ));
            let val_str = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push_str(&val_str);
            out.push('\n');
        }
        out.push_str("--boundary--\n");
    }
    out.trim_end().to_string()
}

fn response_rows(
    op: &Operation,
    namespace: &str,
    version: &str,
    base_components_rel: &str,
) -> Vec<ResponseRow> {
    let mut rows = Vec::new();
    let empty_map = BTreeMap::new();
    let responses = op.responses.as_ref().unwrap_or(&empty_map);
    
    for (status, response_ref) in responses {
        let (desc, headers, content) =
            extract_response_details(response_ref, namespace, version, base_components_rel);
        rows.push(ResponseRow {
            status: status.clone(),
            description: desc,
            headers,
            content,
        });
    }
    rows
}

fn extract_response_details(
    response_ref: &ObjectOrReference<Response>,
    namespace: &str,
    version: &str,
    base_components_rel: &str,
) -> (String, Vec<ParamRow>, String) {
    match response_ref {
        ObjectOrReference::Ref { ref_path: reference, .. } => {
            (format!("Reference: {}", ref_to_link(reference, base_components_rel)), Vec::new(), String::new())
        }
        ObjectOrReference::Object(item) => {
            let mut headers = Vec::new();
            for (name, header_ref) in &item.headers {
                match header_ref {
                    ObjectOrReference::Ref { ref_path: reference, .. } => {
                        headers.push(ParamRow {
                            name: format!("{name} (ref)"),
                            required: "Unknown".to_string(),
                            param_type: ref_to_link(reference, base_components_rel),
                            description: String::new(),
                        });
                    }
                    ObjectOrReference::Object(header) => {
                        headers.push(row_from_header(name, header, base_components_rel));
                    }
                }
            }

            let mut out = String::new();
            for (ctype, media_type) in &item.content {
                out.push('\n');
                out.push_str(&inline_body_doc(
                    "#### Response Schema",
                    ctype,
                    media_type,
                    namespace,
                    version,
                    base_components_rel,
                ));
            }

            (
                item.description.clone().unwrap_or_default(),
                headers,
                out.trim_start().to_string(),
            )
        }
    }
}

fn row_from_header(name: &str, header: &oas3::spec::Header, base_components_rel: &str) -> ParamRow {
    let ty = if let Some(schema_ref) = &header.schema {
        match schema_ref {
            ObjectOrReference::Ref { ref_path: reference, .. } => {
                ref_to_link(reference, base_components_rel)
            }
            ObjectOrReference::Object(schema) => {
                super::components::schema_type_to_string(schema, &format!("{}/schemas", base_components_rel))
            }
        }
    } else if let Some(content) = &header.content {
         if let Some((ctype, _)) = content.iter().next() {
            format!("content `{ctype}`")
        } else {
            "content".to_string()
        }
    } else {
        "any".to_string()
    };

    ParamRow {
        name: name.to_string(),
        required: if header.required.unwrap_or(false) { "Yes" } else { "No" }.to_string(),
        param_type: ty,
        description: header.description.clone().unwrap_or_default(),
    }
}


fn ref_to_link(reference: &str, base_components_rel: &str) -> String {
    let parts: Vec<&str> = reference.split('/').collect();
    if parts.len() >= 4 && parts[0] == "#" && parts[1] == "components" {
        let kind = parts[2];
        let name = parts[3];
        format!("[{name}]({base_components_rel}/{kind}/{name}.md)")
    } else {
        let name = reference.rsplit('/').next().unwrap_or(reference);
        format!("[{name}]({base_components_rel}/schemas/{name}.md)")
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
        "responses": {
          "201": {
            "description": "Created",
            "headers": {
              "X-RateLimit": {
                "schema": { "type": "integer" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "properties": {
                    "new_id": { "type": "string" }
                  }
                }
              }
            }
          }
        }
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
        assert!(rendered.contains("### 201"));
        assert!(rendered.contains("#### Headers"));
        assert!(rendered.contains("X-RateLimit"));
        assert!(rendered.contains("#### Response Schema (`application/json`)"));
        assert!(rendered.contains("new_id"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_dir_all(out_root);
    }

    #[test]
    fn request_body_ref_generates_markdown_link() {
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
        assert!(rendered.contains("[ItemCreate](../_components/schemas/ItemCreate.md)"));
        assert!(!rendered.contains("apix peek"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_dir_all(out_root);
    }

    #[test]
    fn resolves_path_item_references() {
        let spec = r##"{
  "openapi": "3.0.0",
  "info": { "title": "T", "version": "v9" },
  "servers": [{ "url": "https://api.example.com" }],
  "paths": {
    "/target": {
      "get": {
        "summary": "Target Endpoint",
        "responses": { "200": { "description": "OK" } }
      }
    },
    "/link": {
      "$ref": "#/paths/~1target"
    }
  }
}"##;
        let spec_path =
            std::env::temp_dir().join(format!("apix-routes-pathref-{}.json", std::process::id()));
        let out_root =
            std::env::temp_dir().join(format!("apix-routes-pathref-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_root);
        std::fs::write(&spec_path, spec).expect("write");

        let parsed = parse_spec(spec_path.to_str().expect("path")).expect("parse");
        let _ = generate_routes(&parsed, &out_root, "demo").expect("generate");

        // the resolved path item /link should have a GET.md and be identical to /target's GET.md
        let _target_rendered =
            std::fs::read_to_string(out_root.join("target/GET.md")).expect("read target");
        let link_rendered =
            std::fs::read_to_string(out_root.join("link/GET.md")).expect("read link");

        // However, the URL inside the rendered markdown differs slightly since base_path has changed.
        assert!(link_rendered.contains("Target Endpoint"));
        assert!(link_rendered.contains("/link"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_dir_all(out_root);
    }
}
