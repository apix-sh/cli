use crate::config::Config;
use crate::error::ApixError;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ResolvedRoute {
    pub source: String,
    pub namespace: String,
    pub version: String,
    pub relative: PathBuf,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ResolvedNamespace {
    pub source: String,
    pub root: PathBuf,
}

pub fn source_root(source: &str) -> Result<PathBuf, ApixError> {
    Ok(Config::apix_home()?.join("vaults").join(source))
}

pub fn resolve_namespace(
    namespace: &str,
    source_override: Option<&str>,
) -> Result<ResolvedNamespace, ApixError> {
    let cfg = Config::load()?;
    let sources = candidate_sources(&cfg, source_override);
    let mut matches = Vec::new();

    for source in sources {
        let root = source_root(&source)?.join(namespace);
        if root.exists() {
            matches.push(ResolvedNamespace { source, root });
        }
    }

    match matches.len() {
        0 => Err(ApixError::VaultNotFound(namespace.to_string())),
        1 => Ok(matches.remove(0)),
        _ => {
            let names: Vec<String> = matches
                .into_iter()
                .map(|m| format!("{}/{}", m.source, namespace))
                .collect();
            Err(ApixError::Ambiguous(format!(
                "Namespace `{namespace}` exists in multiple sources: {}. Pass --source.",
                names.join(", ")
            )))
        }
    }
}

pub fn resolve_route_path(
    route: &str,
    source_override: Option<&str>,
) -> Result<ResolvedRoute, ApixError> {
    let cfg = Config::load()?;
    let parts: Vec<&str> = route.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 3 {
        return Err(ApixError::Parse(format!(
            "Route must include namespace/version/path, got: {route}"
        )));
    }

    let (forced_source, idx) = parse_source_prefix(&cfg, &parts, source_override);
    let namespace = parts
        .get(idx)
        .ok_or_else(|| ApixError::Parse(format!("Invalid route: {route}")))?;
    let version = parts
        .get(idx + 1)
        .ok_or_else(|| ApixError::Parse(format!("Invalid route: {route}")))?;
    let rel_parts = &parts[(idx + 2)..];
    if rel_parts.is_empty() {
        return Err(ApixError::Parse(format!(
            "Route must include endpoint path and method: {route}"
        )));
    }

    let relative = rel_parts.iter().collect::<PathBuf>();
    let sources = if let Some(s) = forced_source {
        vec![s]
    } else {
        candidate_sources(&cfg, source_override)
    };

    let mut matches = Vec::new();
    for source in sources {
        let mut file = source_root(&source)?
            .join(namespace)
            .join(version)
            .join(&relative);
        file.set_extension("md");
        if file.exists() {
            matches.push(ResolvedRoute {
                source,
                namespace: (*namespace).to_string(),
                version: (*version).to_string(),
                relative: relative.clone(),
                file_path: file,
            });
        }
    }

    match matches.len() {
        0 => Err(ApixError::RouteNotFound(route.to_string())),
        1 => Ok(matches.remove(0)),
        _ => {
            let names: Vec<String> = matches
                .into_iter()
                .map(|m| {
                    format!(
                        "{}/{}/{}/{}",
                        m.source,
                        m.namespace,
                        m.version,
                        m.relative.display()
                    )
                })
                .collect();
            Err(ApixError::Ambiguous(format!(
                "Route `{route}` matched multiple sources: {}. Pass --source or use source-prefixed route.",
                names.join(", ")
            )))
        }
    }
}

pub fn route_resolution_inputs(
    route: &str,
    source_override: Option<&str>,
) -> Result<(Vec<String>, String, String, Vec<String>), ApixError> {
    let cfg = Config::load()?;
    let parts: Vec<&str> = route.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 4 {
        return Err(ApixError::RouteNotFound(route.to_string()));
    }
    let (forced_source, idx) = parse_source_prefix(&cfg, &parts, source_override);
    let namespace = parts[idx].to_string();
    let version = parts[idx + 1].to_string();
    let path_parts = parts[(idx + 2)..]
        .iter()
        .map(|s| (*s).to_string())
        .collect::<Vec<_>>();
    let sources = if let Some(s) = forced_source {
        vec![s]
    } else {
        candidate_sources(&cfg, source_override)
    };
    Ok((sources, namespace, version, path_parts))
}

fn parse_source_prefix(
    cfg: &Config,
    parts: &[&str],
    source_override: Option<&str>,
) -> (Option<String>, usize) {
    if let Some(s) = source_override {
        return (Some(s.to_string()), 0);
    }
    if parts.len() >= 4 && cfg.known_sources().iter().any(|k| k == parts[0]) {
        return (Some(parts[0].to_string()), 1);
    }
    (None, 0)
}

fn candidate_sources(cfg: &Config, source_override: Option<&str>) -> Vec<String> {
    if let Some(s) = source_override {
        return vec![s.to_string()];
    }
    cfg.source_priority()
}

pub fn walk_markdown_under(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(path).hidden(false).build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false)
            && entry.path().extension().and_then(|e| e.to_str()) == Some("md")
        {
            out.push(entry.into_path());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::{remove_var, set_var};
    use serial_test::serial;

    #[test]
    #[serial]
    fn resolves_route_to_local_source_path() {
        let home = std::env::temp_dir().join(format!("apix-resolver-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("vaults/.local/foobar/v1/items/create")).expect("mkdir");
        std::fs::write(
            home.join("vaults/.local/foobar/v1/items/create/POST.md"),
            "# test",
        )
        .expect("write");
        set_var("APIX_HOME", &home);

        let resolved = resolve_route_path("foobar/v1/items/create/POST", None).expect("resolve");
        assert_eq!(resolved.source, ".local");
        assert!(
            resolved
                .file_path
                .to_string_lossy()
                .ends_with("/vaults/.local/foobar/v1/items/create/POST.md")
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn explicit_source_prefix_resolves_correct_source() {
        let home = std::env::temp_dir().join(format!(
            "apix-resolver-test-explicit-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("vaults/core/demo/v1/pets")).expect("mkdir");
        std::fs::create_dir_all(home.join("vaults/.local/demo/v1/pets")).expect("mkdir");
        std::fs::write(home.join("vaults/core/demo/v1/pets/GET.md"), "# core").expect("write");
        std::fs::write(home.join("vaults/.local/demo/v1/pets/GET.md"), "# local").expect("write");
        set_var("APIX_HOME", &home);

        let resolved = resolve_route_path("core/demo/v1/pets/GET", None).expect("resolve");
        assert_eq!(resolved.source, "core");
        assert!(
            resolved
                .file_path
                .to_string_lossy()
                .contains("/vaults/core/demo/v1/pets/GET.md")
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn short_route_is_ambiguous_when_multiple_sources_match() {
        let home =
            std::env::temp_dir().join(format!("apix-resolver-test-amb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("vaults/core/demo/v1/pets")).expect("mkdir");
        std::fs::create_dir_all(home.join("vaults/.local/demo/v1/pets")).expect("mkdir");
        std::fs::write(home.join("vaults/core/demo/v1/pets/GET.md"), "# core").expect("write");
        std::fs::write(home.join("vaults/.local/demo/v1/pets/GET.md"), "# local").expect("write");
        set_var("APIX_HOME", &home);

        let err = resolve_route_path("demo/v1/pets/GET", None).expect_err("must be ambiguous");
        match err {
            ApixError::Ambiguous(msg) => assert!(msg.contains("multiple sources")),
            other => panic!("unexpected error: {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn resolves_from_third_party_source_when_in_priority_list() {
        let home =
            std::env::temp_dir().join(format!("apix-resolver-test-third-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("vaults/acme/demo/v1/pets")).expect("mkdir");
        std::fs::write(home.join("vaults/acme/demo/v1/pets/GET.md"), "# acme").expect("write");
        set_var("APIX_HOME", &home);
        set_var("APIX_SOURCES", ".local,core,acme");

        let resolved = resolve_route_path("demo/v1/pets/GET", None).expect("resolve");
        assert_eq!(resolved.source, "acme");

        remove_var("APIX_SOURCES");
        let _ = std::fs::remove_dir_all(&home);
    }
}
