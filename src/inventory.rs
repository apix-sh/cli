use crate::config::Config;
use crate::error::ApixError;
use crate::output;
use crate::vault::frontmatter::{Frontmatter, extract_frontmatter};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
struct NamespaceRecord {
    source: String,
    namespace: String,
    versions: Vec<VersionRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct VersionRecord {
    version: String,
    route_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LsTarget {
    All,
    Namespace(String),
    NamespaceVersion {
        namespace: String,
        version: String,
        path_prefix: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
struct RouteEntry {
    method: String,
    summary: String,
}

#[derive(Debug, Clone)]
struct ResolvedVersion {
    source: String,
    version_root: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct NamespaceVersionDetail {
    source: String,
    namespace: String,
    version: String,
    routes: Vec<RouteGroup>,
}

#[derive(Debug, Clone, Serialize)]
struct RouteGroup {
    path: String,
    routes: Vec<RouteEntry>,
}

pub fn ls(namespace: Option<&str>, source_override: Option<&str>) -> Result<(), ApixError> {
    match parse_ls_target(namespace)? {
        LsTarget::All => {
            let records = scan_local_inventory(None, source_override)?;
            if records.is_empty() {
                if output::options().json {
                    println!("[]");
                    return Ok(());
                }
                println!("No local namespaces found.");
                println!("Try: apix import <spec> --name <namespace> or apix pull <namespace>");
                return Ok(());
            }
            if output::options().json {
                print_json(&records)?;
                return Ok(());
            }
            print_grouped_by_source(&records);
            Ok(())
        }
        LsTarget::Namespace(ns) => {
            let records = scan_local_inventory(Some(&ns), source_override)?;
            if records.is_empty() {
                if output::options().json {
                    println!("[]");
                    return Ok(());
                }
                println!("No local namespace named `{ns}` found.");
                println!("Try: apix search {ns} --all-sources");
                return Ok(());
            }
            if output::options().json {
                print_json(&records)?;
                return Ok(());
            }
            print_namespace_detail(&records);
            Ok(())
        }
        LsTarget::NamespaceVersion { namespace, version, path_prefix } => {
            if output::options().json {
                let detail = namespace_version_detail(&namespace, &version, path_prefix.as_deref(), source_override)?;
                print_json(&detail)?;
                return Ok(());
            }
            let out = render_namespace_version_detail(&namespace, &version, path_prefix.as_deref(), source_override)?;
            output::print_with_optional_pager(&out);
            Ok(())
        }
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<(), ApixError> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|e| ApixError::Parse(format!("Failed to render JSON output: {e}")))?;
    println!("{rendered}");
    Ok(())
}

fn parse_ls_target(value: Option<&str>) -> Result<LsTarget, ApixError> {
    let Some(raw) = value else {
        return Ok(LsTarget::All);
    };
    if raw.contains('/') {
        let parts: Vec<&str> = raw.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 {
            return Err(ApixError::Parse(
                "`apix ls` route-detail mode expects at least `<namespace>/<version>`".to_string(),
            ));
        }
        let namespace = parts[0].to_string();
        let version = parts[1].to_string();
        let path_prefix = if parts.len() > 2 {
            Some(parts[2..].join("/"))
        } else {
            None
        };
        return Ok(LsTarget::NamespaceVersion {
            namespace,
            version,
            path_prefix,
        });
    }
    Ok(LsTarget::Namespace(raw.to_string()))
}

fn render_namespace_version_detail(
    namespace: &str,
    version: &str,
    path_prefix: Option<&str>,
    source_override: Option<&str>,
) -> Result<String, ApixError> {
    let detail = namespace_version_detail(namespace, version, path_prefix, source_override)?;
    let mut out = String::new();
    out.push_str(&format!(
        "{}/{} (source: {})\n\n",
        output::fmt_namespace(namespace), version, output::fmt_source(&detail.source)
    ));

    for group in detail.routes {
        out.push_str(&format!("{}\n", output::fmt_path(&group.path)));
        for entry in group.routes {
            out.push_str(&format!("  {}: {}\n", output::fmt_method(&entry.method), entry.summary));
        }
    }

    Ok(out)
}

fn namespace_version_detail(
    namespace: &str,
    version: &str,
    path_prefix: Option<&str>,
    source_override: Option<&str>,
) -> Result<NamespaceVersionDetail, ApixError> {
    let resolved = resolve_version_root(namespace, version, source_override)?;
    let mut grouped = collect_route_groups(&resolved.version_root)?;

    if let Some(prefix) = path_prefix {
        grouped.retain(|path, _| path == prefix || path.starts_with(&format!("{prefix}/")));
    }

    if grouped.is_empty() {
        if let Some(prefix) = path_prefix {
            return Err(ApixError::RouteNotFound(format!(
                "No route markdown files found for `{namespace}/{version}/{prefix}` in source `{}`",
                resolved.source
            )));
        } else {
            return Err(ApixError::RouteNotFound(format!(
                "No route markdown files found for `{namespace}/{version}` in source `{}`",
                resolved.source
            )));
        }
    }

    let routes = grouped
        .into_iter()
        .map(|(path, routes)| RouteGroup { path, routes })
        .collect();
    Ok(NamespaceVersionDetail {
        source: resolved.source,
        namespace: namespace.to_string(),
        version: version.to_string(),
        routes,
    })
}

fn resolve_version_root(
    namespace: &str,
    version: &str,
    source_override: Option<&str>,
) -> Result<ResolvedVersion, ApixError> {
    let cfg = Config::load()?;
    let sources = if let Some(source) = source_override {
        vec![source.to_string()]
    } else {
        cfg.source_priority()
    };

    for source in sources {
        let version_root = Config::apix_home()?
            .join("vaults")
            .join(&source)
            .join(namespace)
            .join(version);
        if version_root.join("_metadata.md").exists() {
            return Ok(ResolvedVersion {
                source,
                version_root,
            });
        }
    }

    if let Some(source) = source_override {
        return Err(ApixError::VaultNotFound(format!(
            "{namespace}/{version} not found in source `{source}`"
        )));
    }

    Err(ApixError::VaultNotFound(format!("{namespace}/{version}")))
}

fn collect_route_groups(
    version_root: &Path,
) -> Result<BTreeMap<String, Vec<RouteEntry>>, ApixError> {
    let mut grouped: BTreeMap<String, Vec<RouteEntry>> = BTreeMap::new();

    for file in crate::vault::resolver::walk_markdown_under(version_root) {
        let rel = match file.strip_prefix(version_root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy();
        if rel_str == "_metadata.md" || rel_str.starts_with("_components/") {
            continue;
        }

        let Some(stem) = rel.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let method = stem.to_ascii_uppercase();
        if method.is_empty() {
            continue;
        }

        let path = rel
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        let summary = extract_route_summary(&file)?;
        grouped
            .entry(path)
            .or_default()
            .push(RouteEntry { method, summary });
    }

    for entries in grouped.values_mut() {
        entries.sort_by(|a, b| {
            method_order_key(&a.method)
                .cmp(&method_order_key(&b.method))
                .then_with(|| a.method.cmp(&b.method))
        });
    }

    Ok(grouped)
}

fn extract_route_summary(path: &Path) -> Result<String, ApixError> {
    let content = std::fs::read_to_string(path)?;
    let body = match extract_frontmatter::<Frontmatter>(&content) {
        Ok((_fm, body)) => body,
        Err(_) => content.as_str(),
    };
    Ok(extract_summary_from_body(body))
}

fn extract_summary_from_body(body: &str) -> String {
    let mut in_code_fence = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            continue;
        }
        if trimmed.starts_with('#') {
            let title = trimmed.trim_start_matches('#').trim();
            if !title.is_empty() {
                return truncate_summary(title);
            }
            continue;
        }
        if trimmed.starts_with("|") {
            continue;
        }
        return truncate_summary(trimmed);
    }
    "(no description)".to_string()
}

fn truncate_summary(input: &str) -> String {
    let max = 120usize;
    if input.chars().count() <= max {
        return input.to_string();
    }
    let mut out = input.chars().take(max - 3).collect::<String>();
    out.push_str("...");
    out
}

fn method_order_key(method: &str) -> usize {
    match method {
        "GET" => 0,
        "POST" => 1,
        "PUT" => 2,
        "PATCH" => 3,
        "DELETE" => 4,
        "OPTIONS" => 5,
        "HEAD" => 6,
        _ => 99,
    }
}

fn scan_local_inventory(
    namespace_filter: Option<&str>,
    source_override: Option<&str>,
) -> Result<Vec<NamespaceRecord>, ApixError> {
    let cfg = Config::load()?;
    let sources = if let Some(s) = source_override {
        vec![s.to_string()]
    } else {
        cfg.source_priority()
    };

    let mut out = Vec::new();
    for source in sources {
        let source_root = Config::apix_home()?.join("vaults").join(&source);
        if !source_root.exists() {
            continue;
        }
        for ns in read_namespaces(&source_root)? {
            if let Some(filter) = namespace_filter
                && ns != filter {
                    continue;
                }
            let versions = read_versions_with_route_counts(&source_root.join(&ns))?;
            if versions.is_empty() {
                continue;
            }
            out.push(NamespaceRecord {
                source: source.clone(),
                namespace: ns,
                versions,
            });
        }
    }

    Ok(out)
}

fn read_namespaces(source_root: &Path) -> Result<Vec<String>, ApixError> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(source_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ignored_namespace_dir(&name) {
            continue;
        }
        out.push(name);
    }
    out.sort();
    Ok(out)
}

fn read_versions_with_route_counts(namespace_root: &Path) -> Result<Vec<VersionRecord>, ApixError> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(namespace_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let version = entry.file_name().to_string_lossy().to_string();
        let version_root = entry.path();
        if !version_root.join("_metadata.md").exists() {
            continue;
        }
        out.push(VersionRecord {
            version,
            route_count: count_routes(&version_root),
        });
    }
    out.sort_by(|a, b| a.version.cmp(&b.version));
    Ok(out)
}

fn count_routes(version_root: &Path) -> usize {
    crate::vault::resolver::walk_markdown_under(version_root)
        .into_iter()
        .filter(|p| {
            let rel = match p.strip_prefix(version_root) {
                Ok(r) => r,
                Err(_) => return false,
            };
            let s = rel.to_string_lossy();
            !s.starts_with("_components/") && s != "_metadata.md"
        })
        .count()
}

fn print_grouped_by_source(records: &[NamespaceRecord]) {
    let mut current: Option<&str> = None;
    for ns in records {
        if current != Some(ns.source.as_str()) {
            current = Some(ns.source.as_str());
            println!("{}", output::fmt_source(&ns.source));
        }
        println!("  {} ({})", output::fmt_namespace(&ns.namespace), format_versions(&ns.versions));
    }
}

fn print_namespace_detail(records: &[NamespaceRecord]) {
    println!("{}", output::fmt_namespace(&records[0].namespace));
    for r in records {
        println!(
            "  {:<10} versions: {}",
            output::fmt_source(&r.source),
            format_versions_detailed(&r.versions)
        );
    }
}

fn format_versions(versions: &[VersionRecord]) -> String {
    versions
        .iter()
        .map(|v| v.version.clone())
        .collect::<Vec<_>>()
        .join(",")
}

fn format_versions_detailed(versions: &[VersionRecord]) -> String {
    versions
        .iter()
        .map(|v| format!("{} (routes: {})", v.version, v.route_count))
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_ignored_namespace_dir(name: &str) -> bool {
    name.starts_with('.') || name == "registry.json"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::set_var;
    use serial_test::serial;

    #[test]
    fn parses_ls_target_variants() {
        assert_eq!(parse_ls_target(None).expect("parse"), LsTarget::All);
        assert_eq!(
            parse_ls_target(Some("demo")).expect("parse"),
            LsTarget::Namespace("demo".to_string())
        );
        assert_eq!(
            parse_ls_target(Some("demo/v1")).expect("parse"),
            LsTarget::NamespaceVersion {
                namespace: "demo".to_string(),
                version: "v1".to_string(),
                path_prefix: None,
            }
        );
        assert_eq!(
            parse_ls_target(Some("too/many/parts/here")).expect("parse"),
            LsTarget::NamespaceVersion {
                namespace: "too".to_string(),
                version: "many".to_string(),
                path_prefix: Some("parts/here".to_string()),
            }
        );
    }

    #[test]
    #[serial]
    fn scans_inventory_across_sources() {
        let home = std::env::temp_dir().join(format!("apix-ls-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        std::fs::create_dir_all(home.join("vaults/.local/pet/v1")).expect("mkdir");
        std::fs::create_dir_all(home.join("vaults/core/pay/v2")).expect("mkdir");
        std::fs::write(home.join("vaults/.local/pet/v1/_metadata.md"), "#").expect("write");
        std::fs::write(home.join("vaults/core/pay/v2/_metadata.md"), "#").expect("write");
        std::fs::create_dir_all(home.join("vaults/.local/pet/v1/pets")).expect("mkdir");
        std::fs::write(home.join("vaults/.local/pet/v1/pets/GET.md"), "x").expect("write");

        let records = scan_local_inventory(None, None).expect("scan");
        assert!(records.iter().any(|r| r.namespace == "pet"));
        assert!(records.iter().any(|r| r.namespace == "pay"));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn import_and_core_layout_are_reflected_in_ls_scan() {
        let home = std::env::temp_dir().join(format!("apix-ls-integ-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/petstore.json");
        crate::build::import(fixture.to_str().expect("path"), "petstore", None, false)
            .expect("import");

        std::fs::create_dir_all(home.join("vaults/core/pay/v2/payments")).expect("mkdir");
        std::fs::write(home.join("vaults/core/pay/v2/_metadata.md"), "# Pay API").expect("write");
        std::fs::write(home.join("vaults/core/pay/v2/payments/GET.md"), "x").expect("write");

        let all = scan_local_inventory(None, None).expect("scan");
        assert!(
            all.iter()
                .any(|r| r.source == ".local" && r.namespace == "petstore")
        );
        assert!(
            all.iter()
                .any(|r| r.source == "core" && r.namespace == "pay")
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn namespace_and_source_filters_work() {
        let home = std::env::temp_dir().join(format!("apix-ls-filter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);
        std::fs::create_dir_all(home.join("vaults/.local/demo/v1")).expect("mkdir");
        std::fs::create_dir_all(home.join("vaults/core/demo/v2")).expect("mkdir");
        std::fs::write(home.join("vaults/.local/demo/v1/_metadata.md"), "#").expect("write");
        std::fs::write(home.join("vaults/core/demo/v2/_metadata.md"), "#").expect("write");

        let one = scan_local_inventory(Some("demo"), Some(".local")).expect("scan");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].source, ".local");

        let both = scan_local_inventory(Some("demo"), None).expect("scan");
        assert_eq!(both.len(), 2);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn route_detail_prefers_first_source_when_not_overridden() {
        let home = std::env::temp_dir().join(format!("apix-ls-route-pref-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        std::fs::create_dir_all(home.join("vaults/.local/demo/v1/items")).expect("mkdir");
        std::fs::create_dir_all(home.join("vaults/core/demo/v1/items")).expect("mkdir");
        std::fs::write(home.join("vaults/.local/demo/v1/_metadata.md"), "#").expect("write");
        std::fs::write(home.join("vaults/core/demo/v1/_metadata.md"), "#").expect("write");
        std::fs::write(
            home.join("vaults/.local/demo/v1/items/GET.md"),
            "---\nmethod: GET\nurl: http://local\n---\n# Local title\n",
        )
        .expect("write");
        std::fs::write(
            home.join("vaults/core/demo/v1/items/GET.md"),
            "---\nmethod: GET\nurl: http://core\n---\n# Core title\n",
        )
        .expect("write");

        let out = render_namespace_version_detail("demo", "v1", None, None).expect("render");
        let plain = String::from_utf8(strip_ansi_escapes::strip(out)).unwrap();
        assert!(plain.contains("source: .local"));
        assert!(plain.contains("GET: Local title"));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn route_detail_respects_source_override() {
        let home = std::env::temp_dir().join(format!("apix-ls-route-src-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        std::fs::create_dir_all(home.join("vaults/.local/demo/v1/items")).expect("mkdir");
        std::fs::create_dir_all(home.join("vaults/core/demo/v1/items")).expect("mkdir");
        std::fs::write(home.join("vaults/.local/demo/v1/_metadata.md"), "#").expect("write");
        std::fs::write(home.join("vaults/core/demo/v1/_metadata.md"), "#").expect("write");
        std::fs::write(
            home.join("vaults/core/demo/v1/items/POST.md"),
            "---\nmethod: POST\nurl: http://core\n---\nCore desc line\n",
        )
        .expect("write");

        let out = render_namespace_version_detail("demo", "v1", None, Some("core")).expect("render");
        let plain = String::from_utf8(strip_ansi_escapes::strip(out)).unwrap();
        assert!(plain.contains("source: core"));
        assert!(plain.contains("POST: Core desc line"));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn route_detail_output_is_sorted_by_path_and_method() {
        let home = std::env::temp_dir().join(format!("apix-ls-route-sort-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        std::fs::create_dir_all(home.join("vaults/.local/demo/v1/beta")).expect("mkdir");
        std::fs::create_dir_all(home.join("vaults/.local/demo/v1/alpha")).expect("mkdir");
        std::fs::write(home.join("vaults/.local/demo/v1/_metadata.md"), "#").expect("write");
        std::fs::write(
            home.join("vaults/.local/demo/v1/beta/POST.md"),
            "---\nmethod: POST\nurl: http://x\n---\n# B post\n",
        )
        .expect("write");
        std::fs::write(
            home.join("vaults/.local/demo/v1/beta/GET.md"),
            "---\nmethod: GET\nurl: http://x\n---\n# B get\n",
        )
        .expect("write");
        std::fs::write(
            home.join("vaults/.local/demo/v1/alpha/DELETE.md"),
            "---\nmethod: DELETE\nurl: http://x\n---\n# A delete\n",
        )
        .expect("write");

        let out = render_namespace_version_detail("demo", "v1", None, None).expect("render");
        let plain = String::from_utf8(strip_ansi_escapes::strip(out)).unwrap();
        let alpha = plain.find("alpha").expect("alpha");
        let beta = plain.find("beta").expect("beta");
        assert!(alpha < beta);

        let get = plain.find("GET: B get").expect("get");
        let post = plain.find("POST: B post").expect("post");
        assert!(get < post);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn summary_fallback_prefers_heading_then_body() {
        let from_heading = extract_summary_from_body("# Route title\n\nDetails");
        assert_eq!(from_heading, "Route title");

        let from_body = extract_summary_from_body("\n\nPlain first line\n| table |");
        assert_eq!(from_body, "Plain first line");

        let none = extract_summary_from_body("\n\n| a | b |\n```\ncode\n```");
        assert_eq!(none, "(no description)");
    }

    #[test]
    fn version_formatting_is_stable() {
        let versions = vec![
            VersionRecord {
                version: "v1".to_string(),
                route_count: 3,
            },
            VersionRecord {
                version: "v2".to_string(),
                route_count: 5,
            },
        ];
        assert_eq!(format_versions(&versions), "v1,v2");
        assert_eq!(
            format_versions_detailed(&versions),
            "v1 (routes: 3), v2 (routes: 5)"
        );
    }
}
