use crate::error::ApixError;
use crate::vault::resolver;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{SearcherBuilder, sinks::UTF8};
use std::cell::RefCell;
use std::path::PathBuf;

pub fn grep(
    namespace: &str,
    query: &str,
    limit: usize,
    source_override: Option<&str>,
) -> Result<(), ApixError> {
    let resolved = resolver::resolve_namespace(namespace, source_override)?;
    let base = resolved.root;

    let files = markdown_files(&base);
    let lines = search_markdown_files(&files, &base, query, limit)?;

    for line in lines {
        println!("{}:{line}", resolved.source);
    }

    Ok(())
}

fn search_markdown_files(
    files: &[PathBuf],
    base: &std::path::Path,
    query: &str,
    limit: usize,
) -> Result<Vec<String>, ApixError> {
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(true)
        .build(&regex::escape(query))
        .map_err(|err| ApixError::Parse(format!("Invalid search query: {err}")))?;
    let mut searcher = SearcherBuilder::new().line_number(true).build();

    let results: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let mut stop = false;

    for file in files {
        if stop {
            break;
        }
        let rel = file
            .strip_prefix(&base)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file.to_string_lossy().to_string());

        let sink = UTF8(|line_num, line| {
            if results.borrow().len() >= limit {
                stop = true;
                return Ok(false);
            }

            let ln = line_num;
            results
                .borrow_mut()
                .push(format!("{rel}:{ln}: {}", line.trim_end()));
            Ok(true)
        });

        searcher.search_path(&matcher, &file, sink).map_err(|err| {
            ApixError::Parse(format!("Search failed for {}: {err}", file.display()))
        })?;
    }

    Ok(results.take())
}

fn markdown_files(base: &std::path::Path) -> Vec<PathBuf> {
    resolver::walk_markdown_under(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_markdown_with_limit() {
        let root = std::env::temp_dir().join(format!("apix-search-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("v1/items")).expect("mkdir");
        std::fs::write(root.join("v1/items/GET.md"), "# Get Item\nItem ID\n").expect("write");
        std::fs::write(
            root.join("v1/items/POST.md"),
            "# Create Item\nItem payload\n",
        )
        .expect("write");

        let files = markdown_files(&root);
        let lines = search_markdown_files(&files, &root, "item", 1).expect("search");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("items"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
