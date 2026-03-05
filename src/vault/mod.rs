pub mod frontmatter;
pub mod resolver;

use crate::error::ApixError;
use crate::output;
use std::path::Path;
pub fn show(route: &str, source_override: Option<&str>) -> Result<(), ApixError> {
    let resolved = match resolver::resolve_route_path(route, source_override) {
        Ok(r) => r,
        Err(ApixError::RouteNotFound(_)) => {
            let suggestions = suggest_routes(route)?;
            let hint = if suggestions.is_empty() {
                String::new()
            } else {
                format!(". Did you mean one of: {}", suggestions.join(", "))
            };
            return Err(ApixError::RouteNotFound(format!("{route}{hint}")));
        }
        Err(err) => return Err(err),
    };
    let content = std::fs::read_to_string(resolved.file_path)?;
    
    let rendered_content = if let Ok((fm, body)) = frontmatter::extract_frontmatter::<serde_yaml::Value>(&content) {
        let table = render_frontmatter_table(&fm);
        if table.is_empty() {
            body.to_string()
        } else {
            format!("{}\n{}", table, body)
        }
    } else {
        content.clone() // fallback if no frontmatter
    };

    let rendered = if resolved.source != ".local" {
        format!("> Source: `{}`\n\n{}", resolved.source, rendered_content)
    } else {
        rendered_content
    };
    output::print_markdown_with_optional_pager(&rendered);
    Ok(())
}
pub fn info(target: &str, source_override: Option<&str>) -> Result<(), ApixError> {
    let route = format!("{target}/_metadata");
    let resolved = match resolver::resolve_route_path(&route, source_override) {
        Ok(r) => r,
        Err(_) => {
            return Err(ApixError::VaultNotFound(format!(
                "Metadata for `{target}` not found. Ensure the namespace and version are correct (e.g., namespace/version)."
            )));
        }
    };
    let content = std::fs::read_to_string(resolved.file_path)?;

    let rendered_content =
        if let Ok((fm, body)) = frontmatter::extract_frontmatter::<serde_yaml::Value>(&content) {
            let table = render_frontmatter_table(&fm);
            if table.is_empty() {
                body.to_string()
            } else {
                format!("{}\n{}", table, body)
            }
        } else {
            content.clone()
        };

    let rendered = if resolved.source != ".local" {
        format!("> Source: `{}`\n\n{}", resolved.source, rendered_content)
    } else {
        rendered_content
    };
    output::print_markdown(&rendered);
    Ok(())
}

pub fn peek(route: &str, source_override: Option<&str>) -> Result<(), ApixError> {
    let resolved = resolver::resolve_route_path(route, source_override)?;
    let content = std::fs::read_to_string(resolved.file_path)?;
    if resolved.relative.starts_with(Path::new("_types")) {
        let (frontmatter, _) =
            frontmatter::extract_frontmatter::<frontmatter::TypeFrontmatter>(&content)?;
        let mut out = String::new();
        out.push_str(&render_frontmatter_table(&frontmatter));
        out.push_str("## Path Parameters\n*(None)*\n\n");
        out.push_str("## Required Request Body Fields\n*(None)*\n");
        if resolved.source != ".local" {
            out = format!("> Source: `{}`\n\n{}", resolved.source, out);
        }
        output::print_markdown(&out);
        return Ok(());
    }

    let (frontmatter, body) =
        frontmatter::extract_frontmatter::<frontmatter::Frontmatter>(&content)?;
    let mut out = String::new();
    out.push_str(&render_frontmatter_table(&frontmatter));

    let path_params = extract_section(body, "## Path Parameters");
    out.push_str("## Path Parameters\n");
    if let Some(section) = path_params {
        out.push_str(section.trim());
        out.push('\n');
    } else {
        out.push_str("*(None)*\n");
    }
    out.push('\n');

    out.push_str("## Required Request Body Fields\n");
    match extract_required_body_rows(body) {
        Some(table) if !table.is_empty() => {
            out.push_str("| Property | Required | Type | Description |\n");
            out.push_str("| :--- | :---: | :--- | :--- |\n");
            for row in table {
                out.push_str(&row);
                out.push('\n');
            }
        }
        _ => out.push_str("*(None)*\n"),
    }
    if resolved.source != ".local" {
        out = format!("> Source: `{}`\n\n{}", resolved.source, out);
    }
    output::print_markdown(&out);
    Ok(())
}

fn extract_section<'a>(body: &'a str, heading: &str) -> Option<&'a str> {
    let start = body.find(heading)?;
    let rest = &body[start + heading.len()..];
    let next_heading = rest.find("\n## ").unwrap_or(rest.len());
    Some(rest[..next_heading].trim())
}

fn extract_required_body_rows(body: &str) -> Option<Vec<String>> {
    let section = extract_section(body, "## Request Body")?;
    let mut rows = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.contains("| Yes |") {
            rows.push(trimmed.to_string());
        }
    }
    Some(rows)
}

fn suggest_routes(route: &str) -> Result<Vec<String>, ApixError> {
    let (sources, namespace, version, _) = resolver::route_resolution_inputs(route, None)?;
    let source = sources
        .first()
        .cloned()
        .unwrap_or_else(|| ".local".to_string());
    let ns_root = resolver::source_root(&source)?
        .join(&namespace)
        .join(&version);
    if !ns_root.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for path in resolver::walk_markdown_under(&ns_root) {
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok(rel) = path.strip_prefix(&ns_root) else {
            continue;
        };
        let mut rel_no_ext = rel.to_path_buf();
        rel_no_ext.set_extension("");
        let candidate = format!(
            "{}/{}/{}/{}",
            source,
            namespace,
            version,
            rel_no_ext.to_string_lossy()
        );
        if candidate.contains(route)
            || route
                .split('/')
                .any(|segment| !segment.is_empty() && candidate.contains(segment))
        {
            out.push(candidate);
        }
        if out.len() >= 5 {
            break;
        }
    }
    Ok(out)
}

fn render_frontmatter_table<T: serde::Serialize>(fm: &T) -> String {
    let mut out = String::new();
    let Ok(serde_json::Value::Object(map)) = serde_json::to_value(fm) else {
        return out;
    };
    if map.is_empty() {
        return out;
    }
    out.push_str("| Metadata | Value |\n| :--- | :--- |\n");
    for (k, v) in map {
        if v.is_null() {
            continue;
        }
        let val_str = match v {
            serde_json::Value::String(s) => s,
            _ => v.to_string(),
        };
        out.push_str(&format!("| {k} | `{val_str}` |\n"));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::set_var;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_info_command_logic() {
        let home = std::env::temp_dir().join(format!("apix-vault-info-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("vaults/.local/demo/v1")).expect("mkdir");
        std::fs::write(
            home.join("vaults/.local/demo/v1/_metadata.md"),
            "---\nbase_url: https://api.demo.com\n---\n# Demo\nTest",
        )
        .expect("write");
        set_var("APIX_HOME", &home);

        let result = info("demo/v1", None);
        assert!(result.is_ok());

        let err = info("demo/v2", None).unwrap_err();
        match err {
            ApixError::VaultNotFound(msg) => {
                assert!(msg.contains("demo/v2"));
            }
            _ => panic!("Expected VaultNotFound, got {:?}", err),
        }

        let _ = std::fs::remove_dir_all(&home);
    }
}

