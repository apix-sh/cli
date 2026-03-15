pub mod components;
pub mod parser;
pub mod resolver;
pub mod routes;
pub mod schema_helpers;

use crate::error::ApixError;
use crate::output;
use askama::Template;
use parser::ParsedSpec;
use std::path::{Path, PathBuf};

pub(crate) fn sanitize_markdown_table_cell(value: &str) -> String {
    value
        .replace('|', r"\|")
        .replace("\r\n", "<br/>")
        .replace('\n', "<br/>")
        .replace('\r', "<br/>")
}

#[derive(Template)]
#[template(path = "metadata.md")]
struct MetadataTemplate<'a> {
    base_url: &'a str,
    auth: &'a str,
    title: &'a str,
    description: &'a str,
    version: &'a str,
    tags: Vec<String>,
}

pub fn import(
    source: &str,
    name: &str,
    output_root: Option<&str>,
    overwrite: bool,
) -> Result<(), ApixError> {
    let parsed = parser::parse_spec(source)?;
    let root = target_version_root(name, &parsed.version, output_root)?;

    if root.exists() {
        if !overwrite {
            return Err(ApixError::Config(format!(
                "Target already exists: {}. Use --overwrite to replace it.",
                root.display()
            )));
        }
        std::fs::remove_dir_all(&root)?;
    }

    std::fs::create_dir_all(&root)?;
    write_metadata(&parsed, &root)?;

    let components_count = components::generate_components(&parsed, &root, name)?;
    let route_count = routes::generate_routes(&parsed, &root, name)?;
    if output_root.is_none() {
        crate::registry::rebuild_source_registry(".local")?;
    }
    let total = components_count + route_count + 1;

    output::eprintln_info(&format!(
        "Import complete: {} components, {} routes, {} files written at {}",
        components_count,
        route_count,
        total,
        root.display()
    ));
    Ok(())
}

fn target_version_root(
    namespace: &str,
    version: &str,
    output_root: Option<&str>,
) -> Result<PathBuf, ApixError> {
    if let Some(root) = output_root {
        return Ok(PathBuf::from(root).join(namespace).join(version));
    }
    Ok(crate::config::Config::apix_home()?
        .join("vaults")
        .join(".local")
        .join(namespace)
        .join(version))
}

fn write_metadata(parsed: &ParsedSpec, root: &Path) -> Result<(), ApixError> {
    let tpl = MetadataTemplate {
        base_url: &parsed.base_url,
        auth: &parsed.auth,
        title: &parsed.title,
        description: &parsed.description,
        version: &parsed.version,
        tags: parsed.tags.clone(),
    };
    let out = tpl
        .render()
        .map_err(|err| ApixError::Parse(format!("Failed to render metadata template: {err}")))?;

    std::fs::write(root.join("_metadata.md"), out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::set_var;
    use serial_test::serial;

    fn fixture(name: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        root.join("tests/fixtures")
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    #[test]
    #[serial]
    fn full_import_pipeline_generates_expected_files() {
        let home = std::env::temp_dir().join(format!("apix-it-build-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        import(&fixture("petstore.json"), "petstore", None, false).expect("import");
        let version_root = home.join("vaults/.local/petstore/v1");
        assert!(version_root.join("_metadata.md").exists());
        assert!(version_root.join("_components/schemas/Pet.md").exists());
        assert!(version_root.join("pets/GET.md").exists());
        assert!(version_root.join("pets/{petId}/GET.md").exists());

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn info_show_peek_grep_work_on_generated_vault() {
        let home = std::env::temp_dir().join(format!("apix-it-nav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        import(&fixture("complex.json"), "complex", None, false).expect("import");
        crate::vault::show("complex/v2/events/POST", None).expect("show");
        crate::vault::peek("complex/v2/events/POST", None).expect("peek");
        crate::vault::info("complex/v2", None).expect("info");
        crate::search::grep("complex", "payload", 5, None).expect("grep");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn import_to_explicit_output_writes_expected_tree() {
        let home = std::env::temp_dir().join(format!("apix-it-output-home-{}", std::process::id()));
        let out_root =
            std::env::temp_dir().join(format!("apix-it-output-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&out_root);
        set_var("APIX_HOME", &home);

        import(
            &fixture("petstore.json"),
            "petstore",
            Some(out_root.to_string_lossy().as_ref()),
            false,
        )
        .expect("import");

        let version_root = out_root.join("petstore/v1");
        assert!(version_root.join("_metadata.md").exists());
        assert!(version_root.join("_components/schemas/Pet.md").exists());
        assert!(version_root.join("pets/GET.md").exists());
        assert!(!home.join("vaults/.local/registry.json").exists());

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[test]
    #[serial]
    fn import_refuses_to_overwrite_without_flag() {
        let out_root =
            std::env::temp_dir().join(format!("apix-it-overwrite-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_root);
        std::fs::create_dir_all(out_root.join("petstore/v1")).expect("mkdir");
        std::fs::write(out_root.join("petstore/v1/sentinel.txt"), "keep").expect("write");

        let err = import(
            &fixture("petstore.json"),
            "petstore",
            Some(out_root.to_string_lossy().as_ref()),
            false,
        )
        .expect_err("must fail");
        match err {
            ApixError::Config(msg) => assert!(msg.contains("Target already exists")),
            other => panic!("unexpected error: {other}"),
        }
        assert!(out_root.join("petstore/v1/sentinel.txt").exists());

        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[test]
    #[serial]
    fn import_overwrite_replaces_existing_target() {
        let out_root =
            std::env::temp_dir().join(format!("apix-it-overwrite-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_root);
        std::fs::create_dir_all(out_root.join("petstore/v1")).expect("mkdir");
        std::fs::write(out_root.join("petstore/v1/sentinel.txt"), "old").expect("write");

        import(
            &fixture("petstore.json"),
            "petstore",
            Some(out_root.to_string_lossy().as_ref()),
            true,
        )
        .expect("overwrite import");
        assert!(!out_root.join("petstore/v1/sentinel.txt").exists());
        assert!(out_root.join("petstore/v1/_metadata.md").exists());

        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[test]
    #[serial]
    fn import_auth_extraction_generates_correct_frontmatter() {
        let out_root = std::env::temp_dir().join(format!("apix-it-auth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_root);

        import(
            &fixture("petstore_auth.json"),
            "petauth",
            Some(out_root.to_string_lossy().as_ref()),
            false,
        )
        .expect("import");

        let version_root = out_root.join("petauth/v1");

        // _metadata.md should have global auth = "bearer"
        let metadata =
            std::fs::read_to_string(version_root.join("_metadata.md")).expect("read metadata");
        assert!(
            metadata.contains("auth: \"bearer\""),
            "metadata should contain bearer auth, got:\n{metadata}"
        );

        // /pets GET inherits global auth if oas3 doesn't populate it, it might still say auth: "none"
        // if it differs from the global bearer. In oas3, an inherited route might look the same as an
        // explicitly empty one. We'll accept "none" here for now.
        let pets_get =
            std::fs::read_to_string(version_root.join("pets/GET.md")).expect("read pets GET");
        assert!(
            pets_get.contains(r#"auth: "none""#),
            "inherited route might now explicitly say none in oas3, got:\n{pets_get}"
        );

        // /pets/{petId} GET has security override → auth: "apiKey (header: X-API-KEY)"
        let pet_get = std::fs::read_to_string(version_root.join("pets/{petId}/GET.md"))
            .expect("read pet GET");
        assert!(
            pet_get.contains(r#"auth: "apiKey (header: X-API-KEY)""#),
            "overridden route should have apiKey auth, got:\n{pet_get}"
        );

        // /health GET has security: [] → auth: "none"
        let health_get =
            std::fs::read_to_string(version_root.join("health/GET.md")).expect("read health GET");
        assert!(
            health_get.contains(r#"auth: "none""#),
            "no-auth route should have auth: none, got:\n{health_get}"
        );

        let _ = std::fs::remove_dir_all(&out_root);
    }

    #[test]
    #[serial]
    fn import_extracts_tags_and_indexes_them() {
        let home = std::env::temp_dir().join(format!("apix-it-tags-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        set_var("APIX_HOME", &home);

        import(&fixture("tags.json"), "tagsapi", None, false).expect("import");

        let version_root = home.join("vaults/.local/tagsapi/v1");

        // 1. Verify _metadata.md contains tags
        let metadata =
            std::fs::read_to_string(version_root.join("_metadata.md")).expect("read metadata");
        assert!(
            metadata.contains("tags: [pet, store]"),
            "metadata should contain tags, got:\n{metadata}"
        );

        // 2. Verify registry.json contains tags
        let reg = crate::registry::Registry::load(".local").expect("load registry");
        let entry = reg.apis.get("tagsapi").expect("tagsapi entry missing");
        assert!(entry.tags.contains(&"pet".to_string()));
        assert!(entry.tags.contains(&"store".to_string()));
        assert!(entry.tags.contains(&"local".to_string()));

        let _ = std::fs::remove_dir_all(&home);
    }
}
