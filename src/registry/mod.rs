pub mod git;

use crate::config::Config;
use crate::error::ApixError;
use crate::output;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const AUTO_UPDATE_LOCK_STALE_SECONDS: u64 = 300;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Registry {
    pub apis: HashMap<String, ApiEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApiEntry {
    pub name: String,
    pub description: String,
    pub versions: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
struct SearchHit {
    source: String,
    name: String,
    description: String,
    versions: Vec<String>,
    tags: Vec<String>,
    score: i32,
}

#[derive(Debug, Clone)]
struct MergedHit {
    name: String,
    description: String,
    sources: BTreeSet<String>,
    versions: BTreeSet<String>,
    tags: BTreeSet<String>,
    best_score: i32,
    first_source_rank: usize,
}

#[derive(Debug, Clone, Serialize)]
struct SearchRow {
    sources: Vec<String>,
    name: String,
    description: String,
    versions: Vec<String>,
    tags: Vec<String>,
}

impl Registry {
    pub fn load(source: &str) -> Result<Self, ApixError> {
        let path = registry_path(source)?;
        let raw = std::fs::read_to_string(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                ApixError::VaultNotFound(format!(
                    "Source registry file not found at {}",
                    path.display()
                ))
            } else {
                ApixError::Io(err)
            }
        })?;
        serde_json::from_str(&raw)
            .map_err(|err| ApixError::Parse(format!("Invalid registry.json: {err}")))
    }
}

pub fn search(
    query: &str,
    source: Option<&str>,
    all_sources: bool,
    no_auto_update: bool,
) -> Result<(), ApixError> {
    let cfg = Config::load()?;
    let source_order = pick_search_sources(&cfg, source, all_sources);
    maybe_auto_update_for_search(&cfg, &source_order, no_auto_update);
    let q = query.to_lowercase();

    let mut hits = Vec::new();
    for src in &source_order {
        let registry = match Registry::load(src) {
            Ok(r) => r,
            Err(ApixError::VaultNotFound(_)) => continue,
            Err(e) => return Err(e),
        };
        hits.extend(search_source(src, &registry, &q));
    }

    let rank: HashMap<String, usize> = source_order
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), i))
        .collect();
    let merged = merge_hits(hits, &rank);

    if output::options().json {
        let rows: Vec<SearchRow> = merged
            .into_iter()
            .map(|m| SearchRow {
                sources: m.sources.into_iter().collect(),
                name: m.name,
                description: m.description,
                versions: m.versions.into_iter().collect(),
                tags: m.tags.into_iter().collect(),
            })
            .collect();
        let rendered = serde_json::to_string_pretty(&rows)
            .map_err(|e| ApixError::Parse(format!("Failed to render JSON output: {e}")))?;
        println!("{rendered}");
        return Ok(());
    }

    for m in merged {
        let sources = m.sources.into_iter().collect::<Vec<_>>().join(",");
        let versions = m.versions.into_iter().collect::<Vec<_>>().join(",");
        let tags = m.tags.into_iter().collect::<Vec<_>>().join(",");
        println!(
            "{:<16} {:<20} {:<32} {:<12} [{}]",
            sources, m.name, m.description, versions, tags
        );
    }

    Ok(())
}

fn maybe_auto_update_for_search(cfg: &Config, sources: &[String], no_auto_update: bool) {
    if no_auto_update || !cfg.auto_update_enabled() {
        return;
    }

    let ttl = cfg.auto_update_ttl_seconds();
    for source in sources {
        if source == ".local" {
            continue;
        }
        if cfg.source_remote(source).is_none() {
            continue;
        }

        let missing_registry = registry_path(source).map(|p| !p.exists()).unwrap_or(false);
        let stale = if ttl == 0 {
            false
        } else {
            is_source_stale(source, ttl)
        };
        if !missing_registry && !stale {
            continue;
        }

        if let Err(err) = auto_update_source_registry(source) {
            output::eprintln_warn(&format!("Auto-update skipped for source `{source}`: {err}"));
        }
    }
}

fn auto_update_source_registry(source: &str) -> Result<(), ApixError> {
    let _lock = match acquire_auto_update_lock(source) {
        Ok(lock) => lock,
        Err(err) => {
            output::eprintln_warn(&format!(
                "Auto-update already in progress for source `{source}`: {err}"
            ));
            return Ok(());
        }
    };
    git::update_registry_metadata_only(source)?;
    ensure_registry_exists(source)?;
    write_last_updated(source, now_unix_seconds())?;
    Ok(())
}

pub fn update(source: Option<&str>, all_sources: bool) -> Result<(), ApixError> {
    if all_sources {
        let cfg = Config::load()?;
        for src in cfg.known_sources() {
            if src == ".local" {
                continue;
            }
            if cfg.source_remote(&src).is_none() {
                continue;
            }
            git::update_registry(&src)?;
            ensure_registry_exists(&src)?;
        }
        return Ok(());
    }

    let src = source.unwrap_or("core");
    git::update_registry(src)?;
    ensure_registry_exists(src)
}

pub fn pull(namespace: &str, source: Option<&str>) -> Result<(), ApixError> {
    let src = source.unwrap_or("core");
    git::pull_namespace(namespace, src)?;
    ensure_registry_exists(src)
}

pub fn source_add(name: &str, remote: &str) -> Result<(), ApixError> {
    git::source_add(name, remote)
}

pub fn source_remove(name: &str) -> Result<(), ApixError> {
    git::source_remove(name)
}

pub fn source_list() -> Result<(), ApixError> {
    git::source_list()
}

pub fn rebuild(source: Option<&str>, path: Option<&str>) -> Result<(), ApixError> {
    match (source, path) {
        (Some(_), Some(_)) => Err(ApixError::Parse(
            "Use either `--source` or `--path` for `registry rebuild`, not both".to_string(),
        )),
        (_, Some(root)) => rebuild_registry_at_path(Path::new(root)),
        (Some(src), None) => rebuild_source_registry(src),
        (None, None) => rebuild_source_registry(".local"),
    }
}

pub fn rebuild_source_registry(source: &str) -> Result<(), ApixError> {
    let source_root = Config::apix_home()?.join("vaults").join(source);
    std::fs::create_dir_all(&source_root)?;
    let registry = build_registry_from_root(&source_root, source == ".local")?;
    write_registry(source, &registry)
}

pub fn rebuild_registry_at_path(root: &Path) -> Result<(), ApixError> {
    std::fs::create_dir_all(root)?;
    let registry = build_registry_from_root(root, false)?;
    write_registry_file(&root.join("registry.json"), &registry)
}

fn build_registry_from_root(root: &Path, local_tags: bool) -> Result<Registry, ApixError> {
    let mut apis: HashMap<String, ApiEntry> = HashMap::new();
    for ns_entry in std::fs::read_dir(root)? {
        let ns_entry = ns_entry?;
        if !ns_entry.file_type()?.is_dir() {
            continue;
        }
        let namespace = ns_entry.file_name().to_string_lossy().to_string();
        if is_ignored_root_dir(&namespace) {
            continue;
        }
        let ns_path = ns_entry.path();
        let mut versions = Vec::new();
        let mut title = namespace.clone();
        let mut description = String::new();

        for ver_entry in std::fs::read_dir(&ns_path)? {
            let ver_entry = ver_entry?;
            if !ver_entry.file_type()?.is_dir() {
                continue;
            }
            let version = ver_entry.file_name().to_string_lossy().to_string();
            let meta = ver_entry.path().join("_metadata.md");
            if !meta.exists() {
                continue;
            }
            versions.push(version);
            if let Ok((t, d)) = parse_metadata_markdown(&meta) {
                if title == namespace && !t.is_empty() {
                    title = t;
                }
                if description.is_empty() && !d.is_empty() {
                    description = d;
                }
            }
        }

        if versions.is_empty() {
            continue;
        }
        versions.sort();
        versions.dedup();

        apis.insert(
            namespace.clone(),
            ApiEntry {
                name: namespace,
                description: if description.is_empty() {
                    title
                } else {
                    description
                },
                versions,
                tags: if local_tags {
                    vec!["local".to_string()]
                } else {
                    vec![]
                },
            },
        );
    }
    Ok(Registry { apis })
}

fn is_ignored_root_dir(name: &str) -> bool {
    name.starts_with('.')
}

fn pick_search_sources(cfg: &Config, source: Option<&str>, all_sources: bool) -> Vec<String> {
    if let Some(s) = source {
        return vec![s.to_string()];
    }
    if all_sources {
        return cfg.known_sources();
    }
    cfg.source_priority()
}

fn search_source(source: &str, registry: &Registry, q: &str) -> Vec<SearchHit> {
    let mut out = Vec::new();
    for entry in registry.apis.values() {
        let name_lc = entry.name.to_lowercase();
        let desc_lc = entry.description.to_lowercase();
        let tags_lc: Vec<String> = entry.tags.iter().map(|t| t.to_lowercase()).collect();
        let score = score_search_match(&name_lc, &desc_lc, &tags_lc, q);
        if score == 0 {
            continue;
        }
        out.push(SearchHit {
            source: source.to_string(),
            name: entry.name.clone(),
            description: entry.description.clone(),
            versions: entry.versions.clone(),
            tags: entry.tags.clone(),
            score,
        });
    }
    out
}

fn score_search_match(name: &str, desc: &str, tags: &[String], q: &str) -> i32 {
    if name == q {
        return 100;
    }
    if contains_word_exact(name, q) {
        return 95;
    }
    if name.starts_with(q) {
        return 90;
    }
    if contains_word_prefix(name, q) {
        return 85;
    }
    if name.contains(q) {
        return 80;
    }
    if contains_word_exact(desc, q) {
        return 60;
    }
    if contains_word_prefix(desc, q) {
        return 55;
    }
    if desc.contains(q) {
        return 50;
    }
    if tags.iter().any(|t| t == q) {
        return 40;
    }
    if tags.iter().any(|t| t.starts_with(q)) {
        return 35;
    }
    if tags.iter().any(|t| t.contains(q)) {
        return 30;
    }
    0
}

fn contains_word_exact(text: &str, q: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| !token.is_empty() && token == q)
}

fn contains_word_prefix(text: &str, q: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| !token.is_empty() && token.starts_with(q))
}

fn merge_hits(hits: Vec<SearchHit>, source_rank: &HashMap<String, usize>) -> Vec<MergedHit> {
    let mut map: HashMap<String, MergedHit> = HashMap::new();
    for h in hits {
        let key = h.name.to_lowercase();
        let rank = *source_rank.get(&h.source).unwrap_or(&usize::MAX);
        let e = map.entry(key).or_insert_with(|| MergedHit {
            name: h.name.clone(),
            description: h.description.clone(),
            sources: BTreeSet::new(),
            versions: BTreeSet::new(),
            tags: BTreeSet::new(),
            best_score: h.score,
            first_source_rank: rank,
        });
        e.sources.insert(h.source);
        for v in h.versions {
            e.versions.insert(v);
        }
        for t in h.tags {
            e.tags.insert(t);
        }
        if h.score > e.best_score {
            e.best_score = h.score;
        }
        if rank < e.first_source_rank {
            e.first_source_rank = rank;
            e.description = h.description;
        }
    }
    let mut out: Vec<MergedHit> = map.into_values().collect();
    out.sort_by(|a, b| {
        a.first_source_rank
            .cmp(&b.first_source_rank)
            .then(b.best_score.cmp(&a.best_score))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

fn ensure_registry_exists(source: &str) -> Result<(), ApixError> {
    let path = registry_path(source)?;
    if path.exists() {
        Ok(())
    } else {
        Err(ApixError::VaultNotFound(format!(
            "Source `{source}` is missing registry index at {}",
            path.display()
        )))
    }
}

fn registry_path(source: &str) -> Result<PathBuf, ApixError> {
    Ok(Config::apix_home()?
        .join("vaults")
        .join(source)
        .join("registry.json"))
}

fn lock_path(source: &str) -> Result<PathBuf, ApixError> {
    Ok(Config::apix_home()?
        .join("vaults")
        .join(source)
        .join(".auto-update.lock"))
}

fn last_updated_path(source: &str) -> Result<PathBuf, ApixError> {
    Ok(Config::apix_home()?
        .join("vaults")
        .join(source)
        .join(".last-updated"))
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_last_updated(source: &str) -> Result<Option<u64>, ApixError> {
    if source == ".local" {
        return Ok(None);
    }
    let path = last_updated_path(source)?;
    let raw = match std::fs::read_to_string(path) {
        Ok(v) => v,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(ApixError::Io(err));
        }
    };
    let ts = raw.trim().parse::<u64>().ok();
    Ok(ts)
}

fn write_last_updated(source: &str, ts: u64) -> Result<(), ApixError> {
    if source == ".local" {
        return Ok(());
    }
    let path = last_updated_path(source)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, ts.to_string())?;
    Ok(())
}

fn is_source_stale(source: &str, ttl_seconds: u64) -> bool {
    if source == ".local" {
        return false;
    }
    if ttl_seconds == 0 {
        return false;
    }
    let last = match read_last_updated(source) {
        Ok(Some(v)) => v,
        _ => return true,
    };
    now_unix_seconds().saturating_sub(last) > ttl_seconds
}

struct UpdateLock {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockFileMeta {
    pid: u32,
    created_at: u64,
}

impl Drop for UpdateLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn acquire_auto_update_lock(source: &str) -> Result<UpdateLock, ApixError> {
    let path = lock_path(source)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    for _ in 0..2 {
        match create_lock_file(&path) {
            Ok(lock) => return Ok(lock),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if !is_lock_stale(&path, now_unix_seconds())? {
                    return Err(ApixError::Io(err));
                }
                let _ = std::fs::remove_file(&path);
            }
            Err(err) => return Err(ApixError::Io(err)),
        }
    }

    Err(ApixError::Git(format!(
        "Failed to acquire auto-update lock for source `{source}`"
    )))
}

fn create_lock_file(path: &Path) -> Result<UpdateLock, std::io::Error> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let meta = LockFileMeta {
        pid: std::process::id(),
        created_at: now_unix_seconds(),
    };
    let raw = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string());
    file.write_all(raw.as_bytes())?;
    file.flush()?;
    Ok(UpdateLock {
        path: path.to_path_buf(),
    })
}

fn is_lock_stale(path: &Path, now: u64) -> Result<bool, ApixError> {
    let file = File::open(path)?;
    let meta: LockFileMeta = match serde_json::from_reader(file) {
        Ok(v) => v,
        Err(_) => return Ok(true),
    };
    Ok(now.saturating_sub(meta.created_at) > AUTO_UPDATE_LOCK_STALE_SECONDS)
}

fn write_registry(source: &str, registry: &Registry) -> Result<(), ApixError> {
    let path = registry_path(source)?;
    write_registry_file(&path, registry)
}

fn write_registry_file(path: &Path, registry: &Registry) -> Result<(), ApixError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(registry)
        .map_err(|e| ApixError::Parse(format!("Failed to serialize registry: {e}")))?;
    std::fs::write(path, json)?;
    Ok(())
}

fn parse_metadata_markdown(path: &Path) -> Result<(String, String), ApixError> {
    let raw = std::fs::read_to_string(path)?;
    let mut title = String::new();
    let mut desc = String::new();
    for line in raw.lines() {
        let t = line.trim();
        if title.is_empty() && t.starts_with("# ") {
            title = t.trim_start_matches("# ").trim().to_string();
            continue;
        }
        if !title.is_empty() && desc.is_empty() {
            if t.is_empty() || t.starts_with("**") || t.starts_with("---") {
                continue;
            }
            desc = t.to_string();
        }
    }
    Ok((title, desc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::{remove_var, set_var};
    use serial_test::serial;
    use std::process::Command;

    #[test]
    #[serial]
    fn local_registry_rebuild_indexes_namespaces() {
        let home = std::env::temp_dir().join(format!("apix-reg-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("vaults/.local/pet/v1")).expect("mkdir");
        std::fs::write(
            home.join("vaults/.local/pet/v1/_metadata.md"),
            "---\nbase_url: https://x\n---\n# Pet API\nPet catalog\n",
        )
        .expect("write");
        set_var("APIX_HOME", &home);

        rebuild_source_registry(".local").expect("rebuild");
        let reg = Registry::load(".local").expect("load");
        assert!(reg.apis.contains_key("pet"));
        assert_eq!(reg.apis["pet"].versions, vec!["v1"]);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn merge_dedupes_same_api_across_sources() {
        let hits = vec![
            SearchHit {
                source: ".local".to_string(),
                name: "petstore".to_string(),
                description: "Local".to_string(),
                versions: vec!["v1".to_string()],
                tags: vec!["local".to_string()],
                score: 80,
            },
            SearchHit {
                source: "core".to_string(),
                name: "petstore".to_string(),
                description: "Core".to_string(),
                versions: vec!["v2".to_string()],
                tags: vec!["rest".to_string()],
                score: 80,
            },
        ];
        let rank = HashMap::from([(".local".to_string(), 0usize), ("core".to_string(), 1usize)]);
        let merged = merge_hits(hits, &rank);
        assert_eq!(merged.len(), 1);
        let m = &merged[0];
        assert!(m.sources.contains(".local"));
        assert!(m.sources.contains("core"));
        assert!(m.versions.contains("v1"));
        assert!(m.versions.contains("v2"));
    }

    #[test]
    #[serial]
    fn import_updates_local_registry_and_search_uses_it() {
        let home = std::env::temp_dir().join(format!("apix-reg-build-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/petstore.json");
        crate::build::import(fixture.to_str().expect("path"), "petstore", None, false)
            .expect("import");

        let local_registry = Registry::load(".local").expect("load local registry");
        assert!(local_registry.apis.contains_key("petstore"));
        search("pet", None, false, true).expect("search");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn search_across_mixed_sources_with_overlap() {
        let home = std::env::temp_dir().join(format!("apix-reg-mixed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/petstore.json");
        crate::build::import(fixture.to_str().expect("path"), "petstore", None, false)
            .expect("import");

        std::fs::create_dir_all(home.join("vaults/core")).expect("mkdir");
        let core_registry = Registry {
            apis: HashMap::from([(
                "petstore".to_string(),
                ApiEntry {
                    name: "petstore".to_string(),
                    description: "Core pet API".to_string(),
                    versions: vec!["v9".to_string()],
                    tags: vec!["core".to_string()],
                },
            )]),
        };
        write_registry("core", &core_registry).expect("write core");

        search("pet", None, true, true).expect("all-source search");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn rebuild_regenerates_local_index_from_vault_files() {
        let home = std::env::temp_dir().join(format!("apix-reg-rebuild-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        std::fs::create_dir_all(home.join("vaults/.local/demo/v1")).expect("mkdir");
        std::fs::write(
            home.join("vaults/.local/demo/v1/_metadata.md"),
            "---\nbase_url: https://api.example.com\nauth: Unknown\n---\n# Demo API\nDemo description\n",
        )
        .expect("write");

        rebuild(Some(".local"), None).expect("rebuild command");
        let reg = Registry::load(".local").expect("load");
        assert!(reg.apis.contains_key("demo"));
        assert_eq!(reg.apis["demo"].versions, vec!["v1"]);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn ttl_staleness_and_local_exclusion() {
        let home = std::env::temp_dir().join(format!("apix-reg-ttl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        std::fs::create_dir_all(home.join("vaults/core")).expect("mkdir");
        assert!(is_source_stale("core", 60));
        write_last_updated("core", now_unix_seconds()).expect("write ts");
        assert!(!is_source_stale("core", 60));
        assert!(!is_source_stale("core", 0));
        assert!(!is_source_stale(".local", 60));

        std::fs::write(home.join("vaults/core/.last-updated"), "bad-ts").expect("write bad ts");
        assert!(is_source_stale("core", 60));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn env_overrides_for_auto_update_are_applied() {
        let cfg = Config::default();
        set_var("APIX_AUTO_UPDATE", "0");
        set_var("APIX_AUTO_UPDATE_TTL_SECONDS", "99");
        assert!(!cfg.auto_update_enabled());
        assert_eq!(cfg.auto_update_ttl_seconds(), 99);
        remove_var("APIX_AUTO_UPDATE");
        remove_var("APIX_AUTO_UPDATE_TTL_SECONDS");
    }

    #[test]
    #[serial]
    fn search_auto_updates_missing_core_registry_and_writes_timestamp() {
        let home = std::env::temp_dir().join(format!("apix-reg-autoup-{}", std::process::id()));
        let remote =
            std::env::temp_dir().join(format!("apix-reg-autoup-remote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&remote);
        std::fs::create_dir_all(&remote).expect("mkdir remote");
        set_var("APIX_HOME", &home);
        set_var("APIX_REGISTRY_URL", remote.to_string_lossy().as_ref());

        run_git_cmd(&remote, ["init"]);
        run_git_cmd(&remote, ["config", "user.name", "apix-test"]);
        run_git_cmd(&remote, ["config", "user.email", "apix@example.com"]);
        run_git_cmd(&remote, ["config", "commit.gpgsign", "false"]);
        std::fs::write(
            remote.join("registry.json"),
            r#"{"apis":{"petstore":{"name":"petstore","description":"Pet API","versions":["v1"],"tags":["pets"]}}}"#,
        )
        .expect("write registry");
        run_git_cmd(&remote, ["add", "registry.json"]);
        run_git_cmd(&remote, ["commit", "-m", "init registry"]);

        search("pet", Some("core"), false, false).expect("search");
        assert!(home.join("vaults/core/registry.json").exists());
        assert!(home.join("vaults/core/.last-updated").exists());

        remove_var("APIX_HOME");
        remove_var("APIX_REGISTRY_URL");
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&remote);
    }

    #[test]
    #[serial]
    fn search_continues_when_auto_update_fails() {
        let home = std::env::temp_dir().join(format!("apix-reg-autofail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);
        set_var("APIX_REGISTRY_URL", "/path/does/not/exist");

        std::fs::create_dir_all(home.join("vaults/core")).expect("mkdir");
        write_registry(
            "core",
            &Registry {
                apis: HashMap::from([(
                    "demo".to_string(),
                    ApiEntry {
                        name: "demo".to_string(),
                        description: "Demo".to_string(),
                        versions: vec!["v1".to_string()],
                        tags: vec![],
                    },
                )]),
            },
        )
        .expect("write");
        write_last_updated("core", 1).expect("stale");

        // Auto-update should fail, but search should still succeed using local registry.
        search("demo", Some("core"), false, false).expect("search");

        remove_var("APIX_HOME");
        remove_var("APIX_REGISTRY_URL");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn rebuild_path_ignores_metadata_and_root_files() {
        let root = std::env::temp_dir().join(format!("apix-vault-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".metadata/policies")).expect("mkdir");
        std::fs::create_dir_all(root.join("petstore/v1/pets")).expect("mkdir");
        std::fs::create_dir_all(root.join("scripts")).expect("mkdir");
        std::fs::write(root.join("README.md"), "# Vault").expect("write");
        std::fs::write(root.join("CONTRIBUTION.md"), "# Contrib").expect("write");
        std::fs::write(
            root.join("petstore/v1/_metadata.md"),
            "---\nbase_url: https://api.example.com\n---\n# Petstore\nPet API\n",
        )
        .expect("write");
        std::fs::write(root.join("petstore/v1/pets/GET.md"), "# List pets").expect("write");

        rebuild(None, Some(root.to_string_lossy().as_ref())).expect("rebuild path");

        let raw = std::fs::read_to_string(root.join("registry.json")).expect("read registry");
        let reg: Registry = serde_json::from_str(&raw).expect("parse registry");
        assert!(reg.apis.contains_key("petstore"));
        assert!(!reg.apis.contains_key("scripts"));
        assert!(!reg.apis.contains_key(".metadata"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[serial]
    fn source_and_path_rebuild_are_equivalent_for_non_local_source() {
        let home = std::env::temp_dir().join(format!("apix-reg-eq-home-{}", std::process::id()));
        let root = std::env::temp_dir().join(format!("apix-reg-eq-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&root);
        set_var("APIX_HOME", &home);

        std::fs::create_dir_all(home.join("vaults/core/petstore/v1/pets")).expect("mkdir");
        std::fs::create_dir_all(root.join("petstore/v1/pets")).expect("mkdir");
        let metadata = "---\nbase_url: https://api.example.com\n---\n# Petstore\nPet API\n";
        std::fs::write(home.join("vaults/core/petstore/v1/_metadata.md"), metadata).expect("write");
        std::fs::write(root.join("petstore/v1/_metadata.md"), metadata).expect("write");
        std::fs::write(
            home.join("vaults/core/petstore/v1/pets/GET.md"),
            "# List pets",
        )
        .expect("write");
        std::fs::write(root.join("petstore/v1/pets/GET.md"), "# List pets").expect("write");

        rebuild(Some("core"), None).expect("rebuild source");
        rebuild(None, Some(root.to_string_lossy().as_ref())).expect("rebuild path");

        let source_raw =
            std::fs::read_to_string(home.join("vaults/core/registry.json")).expect("read source");
        let path_raw = std::fs::read_to_string(root.join("registry.json")).expect("read path");
        let source_reg: Registry = serde_json::from_str(&source_raw).expect("parse source");
        let path_reg: Registry = serde_json::from_str(&path_raw).expect("parse path");
        assert_eq!(source_reg.apis, path_reg.apis);

        remove_var("APIX_HOME");
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_lock_detection_uses_timestamp() {
        let root = std::env::temp_dir().join(format!("apix-lock-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let path = root.join(".auto-update.lock");

        let stale = LockFileMeta {
            pid: 123,
            created_at: now_unix_seconds().saturating_sub(AUTO_UPDATE_LOCK_STALE_SECONDS + 1),
        };
        std::fs::write(
            &path,
            serde_json::to_string(&stale).expect("serialize stale lock"),
        )
        .expect("write stale lock");
        assert!(is_lock_stale(&path, now_unix_seconds()).expect("stale check"));

        let fresh = LockFileMeta {
            pid: 123,
            created_at: now_unix_seconds(),
        };
        std::fs::write(
            &path,
            serde_json::to_string(&fresh).expect("serialize fresh lock"),
        )
        .expect("write fresh lock");
        assert!(!is_lock_stale(&path, now_unix_seconds()).expect("fresh check"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_lock_file_is_treated_as_stale() {
        let root = std::env::temp_dir().join(format!("apix-lock-invalid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let path = root.join(".auto-update.lock");
        std::fs::write(&path, "not-json").expect("write");

        assert!(is_lock_stale(&path, now_unix_seconds()).expect("stale check"));

        let _ = std::fs::remove_dir_all(&root);
    }

    fn run_git_cmd<const N: usize>(cwd: &std::path::Path, args: [&str; N]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git command failed");
    }
}
