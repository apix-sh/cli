use crate::error::ApixError;
use crate::output;
use std::process::Command;

pub fn update_registry(source: &str) -> Result<(), ApixError> {
    let config = crate::config::Config::load()?;
    let remote = config
        .source_remote(source)
        .ok_or_else(|| ApixError::Config(format!("No remote configured for source `{source}`")))?;
    let core = crate::config::Config::apix_home()?
        .join("vaults")
        .join(source);

    if !core.exists() || !core.join(".git").exists() {
        if core.exists() {
            std::fs::remove_dir_all(&core)?;
        }
        std::fs::create_dir_all(
            core.parent()
                .ok_or_else(|| ApixError::Git("Invalid registry path".to_string()))?,
        )?;
        run_git(
            [
                "clone",
                "--filter=blob:none",
                "--sparse",
                &remote,
                core.to_string_lossy().as_ref(),
            ],
            None,
        )?;
        run_git(
            ["sparse-checkout", "set", "--no-cone", "registry.json"],
            Some(&core),
        )?;
    } else {
        restore_local_changes(&core)?;
        run_git(["pull"], Some(&core))?;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    output::eprintln_info(&format!("Source `{source}` updated at unix_ts={now}"));
    Ok(())
}

pub fn update_registry_metadata_only(source: &str) -> Result<(), ApixError> {
    let config = crate::config::Config::load()?;
    let remote = config
        .source_remote(source)
        .ok_or_else(|| ApixError::Config(format!("No remote configured for source `{source}`")))?;
    let root = crate::config::Config::apix_home()?
        .join("vaults")
        .join(source);

    if !root.exists() || !root.join(".git").exists() {
        if root.exists() {
            std::fs::remove_dir_all(&root)?;
        }
        std::fs::create_dir_all(
            root.parent()
                .ok_or_else(|| ApixError::Git("Invalid registry path".to_string()))?,
        )?;
        run_git(
            [
                "clone",
                "--filter=blob:none",
                "--sparse",
                &remote,
                root.to_string_lossy().as_ref(),
            ],
            None,
        )?;
        run_git(
            ["sparse-checkout", "set", "--no-cone", "registry.json"],
            Some(&root),
        )?;
        return Ok(());
    }

    restore_local_changes(&root)?;
    run_git(["pull"], Some(&root))?;
    Ok(())
}

pub fn pull_namespace(namespace_arg: &str, source: &str) -> Result<(), ApixError> {
    let core = crate::config::Config::apix_home()?
        .join("vaults")
        .join(source);
    if !core.exists() {
        return Err(ApixError::Git(format!(
            "Source `{source}` not found. Run `apix update --source {source}` first."
        )));
    }

    let (namespace, version) = match namespace_arg.split_once('/') {
        Some((ns, ver)) => (ns, Some(ver)),
        None => (namespace_arg, None),
    };

    let registry = super::Registry::load(source)?;
    if !registry.apis.contains_key(namespace) {
        let suggestions: Vec<String> = registry
            .apis
            .keys()
            .filter(|name| name.contains(namespace) || namespace.contains(*name))
            .take(5)
            .cloned()
            .collect();
        let msg = if suggestions.is_empty() {
            format!("Namespace `{namespace}` not found in registry")
        } else {
            format!(
                "Namespace `{namespace}` not found in registry. Did you mean: {}",
                suggestions.join(", ")
            )
        };
        return Err(ApixError::VaultNotFound(msg));
    }

    if let Some(ver) = version
        && !registry.apis[namespace].versions.contains(&ver.to_string())
    {
        let available_versions = registry.apis[namespace].versions.join(", ");
        return Err(ApixError::VaultNotFound(format!(
            "Version `{ver}` not found in namespace `{namespace}`. Available versions: {available_versions}"
        )));
    }

    let checkout_path = if let Some(ver) = version {
        format!("{namespace}/{ver}/")
    } else {
        format!("{namespace}/")
    };

    restore_local_changes(&core)?;
    run_git(["sparse-checkout", "add", &checkout_path], Some(&core))?;
    run_git(["pull"], Some(&core))?;

    let ns_dir = if let Some(ver) = version {
        core.join(namespace).join(ver)
    } else {
        core.join(namespace)
    };

    let (count, bytes) = summarize_dir(&ns_dir)?;

    let pull_desc = if let Some(ver) = version {
        format!("{source}/{namespace}/{ver}")
    } else {
        format!("{source}/{namespace}")
    };

    output::eprintln_info(&format!(
        "Pulled `{pull_desc}`: {} files, {:.2} MB",
        count,
        bytes as f64 / (1024.0 * 1024.0)
    ));
    Ok(())
}

pub fn source_add(name: &str, remote: &str) -> Result<(), ApixError> {
    crate::config::Config::validate_source_name(name)?;
    let mut cfg = crate::config::Config::load()?;
    cfg.source.insert(
        name.to_string(),
        crate::config::SourceConfig {
            remote: remote.to_string(),
        },
    );
    if !cfg.sources.iter().any(|s| s == name) {
        cfg.sources.push(name.to_string());
    }
    cfg.save()?;
    output::eprintln_info(&format!("Added source `{name}` -> {remote}"));
    Ok(())
}

pub fn source_remove(name: &str) -> Result<(), ApixError> {
    crate::config::Config::validate_source_name(name)?;
    let mut cfg = crate::config::Config::load()?;
    cfg.source.remove(name);
    cfg.sources.retain(|s| s != name);
    cfg.save()?;
    output::eprintln_info(&format!("Removed source `{name}`"));
    Ok(())
}

pub fn source_list() -> Result<(), ApixError> {
    let cfg = crate::config::Config::load()?;
    let order = cfg.source_priority();
    for s in order {
        let remote = cfg
            .source_remote(&s)
            .unwrap_or_else(|| "(no remote configured)".to_string());
        println!("{:<12} {}", output::fmt_source(&s), remote);
    }
    Ok(())
}

fn run_git<const N: usize>(
    args: [&str; N],
    cwd: Option<&std::path::Path>,
) -> Result<(), ApixError> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd
        .output()
        .map_err(|err| ApixError::Git(format!("Failed to execute git {:?}: {err}", args)))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(ApixError::Git(format!(
        "git {:?} failed: {}",
        args,
        stderr.trim()
    )))
}

fn run_git_capture<const N: usize>(
    args: [&str; N],
    cwd: Option<&std::path::Path>,
) -> Result<String, ApixError> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd
        .output()
        .map_err(|err| ApixError::Git(format!("Failed to execute git {:?}: {err}", args)))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(ApixError::Git(format!(
        "git {:?} failed: {}",
        args,
        stderr.trim()
    )))
}

fn restore_local_changes(root: &std::path::Path) -> Result<(), ApixError> {
    if !root.join(".git").exists() {
        return Ok(());
    }
    let status = run_git_capture(["status", "--porcelain"], Some(root))?;
    let has_changes = status.lines().any(|line| {
        let t = line.trim();
        if t.is_empty() {
            return false;
        }
        // Ignore our own untracked metadata files
        if t == "?? .auto-update.lock" || t == "?? .last-updated" {
            return false;
        }
        true
    });

    if has_changes {
        output::eprintln_warn("Discarding local changes to vault before sync...");
        run_git(["reset", "--hard", "HEAD"], Some(root))?;
        run_git(["clean", "-fd"], Some(root))?;
    }
    Ok(())
}

fn summarize_dir(root: &std::path::Path) -> Result<(usize, u64), ApixError> {
    if !root.exists() {
        return Ok((0, 0));
    }
    let mut files = 0usize;
    let mut bytes = 0u64;
    for entry in ignore::WalkBuilder::new(root).hidden(false).build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.file_type().map(|f| f.is_file()).unwrap_or(false) {
            files += 1;
            bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok((files, bytes))
}
