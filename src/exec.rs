use crate::error::ApixError;
use crate::output;
use crate::vault::frontmatter::{Frontmatter, extract_frontmatter};
use crate::vault::resolver;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

#[allow(clippy::too_many_arguments)]
pub fn call(
    route: String,
    headers: Vec<String>,
    data: Option<String>,
    params: Vec<String>,
    query: Vec<String>,
    verbose: bool,
    dry_run: bool,
    source_override: Option<&str>,
) -> Result<(), ApixError> {
    let (path, mut route_params, resolved_source) = resolve_route_file(&route, source_override)?;
    for kv in params {
        let (k, v) = split_kv(&kv, "param")?;
        route_params.insert(k.to_string(), v.to_string());
    }

    let content = std::fs::read_to_string(path)?;
    let (frontmatter, _) = extract_frontmatter::<Frontmatter>(&content)?;

    let mut url = frontmatter.url.clone();
    for (k, v) in &route_params {
        let placeholder = format!("{{{k}}}");
        url = url.replace(&placeholder, v);
    }
    if url.contains('{') && url.contains('}') {
        return Err(ApixError::Parse(format!(
            "Unresolved path parameters in URL: {url}"
        )));
    }
    url = apply_query(url, &query)?;

    let mut req = ureq::request(&frontmatter.method, &url);
    let mut debug_headers: Vec<(String, String)> = Vec::new();
    if let Some(content_type) = &frontmatter.content_type {
        req = req.set("Content-Type", content_type);
        debug_headers.push(("Content-Type".to_string(), content_type.clone()));
    }
    for h in headers {
        let (k, v) = split_header(&h)?;
        req = req.set(k, v);
        debug_headers.push((k.to_string(), v.to_string()));
    }

    let body = read_body(data)?;
    if dry_run {
        return print_dry_run(
            &frontmatter.method,
            &url,
            &resolved_source,
            &debug_headers,
            &body,
        );
    }
    if verbose {
        eprintln!(
            "> {} {} [source={}]",
            frontmatter.method, url, resolved_source
        );
        for (k, v) in &debug_headers {
            eprintln!("> {k}: {v}");
        }
    }

    let response = match body {
        Some(b) => req.send_string(&b),
        None => req.call(),
    };

    match response {
        Ok(resp) => {
            let status = resp.status();
            let status_text = resp.status_text().to_string();
            let headers = response_headers(&resp);
            if verbose {
                eprintln!("< {} {}", status, status_text);
                for name in resp.headers_names() {
                    if let Some(value) = resp.header(&name) {
                        eprintln!("< {name}: {value}");
                    }
                }
            }
            let out = crate::http::read_response(resp)?;
            print_call_success(status, &status_text, headers, out)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let status_text = resp.status_text().to_string();
            let headers = response_headers(&resp);
            eprintln!("HTTP {} {}", code, status_text);
            let body = crate::http::read_response(resp)
                .map_err(|err| ApixError::Http(format!("Failed to read error body: {err}")))?;
            print_call_error(code, &status_text, headers, body)?;
            Err(ApixError::Http(format!("HTTP status {code}")))
        }
        Err(err) => Err(ApixError::Http(format!("Request failed: {err}"))),
    }
}

fn split_kv<'a>(input: &'a str, flag: &str) -> Result<(&'a str, &'a str), ApixError> {
    input.split_once('=').ok_or_else(|| {
        ApixError::Parse(format!(
            "Invalid --{flag} value: {input}. Expected key=value"
        ))
    })
}

fn response_headers(resp: &ureq::Response) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for name in resp.headers_names() {
        if let Some(value) = resp.header(&name) {
            out.push((name, value.to_string()));
        }
    }
    out
}

fn print_call_success(
    status: u16,
    status_text: &str,
    headers: Vec<(String, String)>,
    body: String,
) -> Result<(), ApixError> {
    if output::options().json {
        let payload = CallResponsePayload {
            ok: true,
            status,
            status_text: status_text.to_string(),
            headers,
            body,
        };
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|e| ApixError::Parse(format!("Failed to render JSON output: {e}")))?;
        println!("{rendered}");
        return Ok(());
    }

    print!("{body}");
    Ok(())
}

fn print_call_error(
    status: u16,
    status_text: &str,
    headers: Vec<(String, String)>,
    body: String,
) -> Result<(), ApixError> {
    if output::options().json {
        let payload = CallResponsePayload {
            ok: false,
            status,
            status_text: status_text.to_string(),
            headers,
            body,
        };
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|e| ApixError::Parse(format!("Failed to render JSON output: {e}")))?;
        println!("{rendered}");
        return Ok(());
    }

    print!("{body}");
    Ok(())
}

fn print_dry_run(
    method: &str,
    url: &str,
    source: &str,
    headers: &[(String, String)],
    body: &Option<String>,
) -> Result<(), ApixError> {
    if output::options().json {
        let payload = DryRunPayload {
            source: source.to_string(),
            method: method.to_string(),
            url: url.to_string(),
            headers: headers.to_vec(),
            body: body.clone(),
        };
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|e| ApixError::Parse(format!("Failed to render JSON output: {e}")))?;
        println!("{rendered}");
        return Ok(());
    }

    println!("{method} {url} [source={source}]");
    for (k, v) in headers {
        println!("{k}: {v}");
    }
    if let Some(body) = body {
        if !headers.is_empty() {
            println!();
        }
        print!("{body}");
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct DryRunPayload {
    source: String,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

#[derive(Debug, Serialize)]
struct CallResponsePayload {
    ok: bool,
    status: u16,
    status_text: String,
    headers: Vec<(String, String)>,
    body: String,
}

fn split_header(input: &str) -> Result<(&str, &str), ApixError> {
    let (k, v) = input.split_once(':').ok_or_else(|| {
        ApixError::Parse(format!("Invalid header {input}. Expected 'Key: Value'"))
    })?;
    Ok((k.trim(), v.trim()))
}

fn apply_query(mut url: String, query: &[String]) -> Result<String, ApixError> {
    if query.is_empty() {
        return Ok(url);
    }

    let mut parsed = url::Url::parse(&url)
        .map_err(|err| ApixError::Parse(format!("Invalid URL {url}: {err}")))?;
    for kv in query {
        let (k, v) = split_kv(kv, "query")?;
        parsed.query_pairs_mut().append_pair(k, v);
    }
    url.clear();
    url.push_str(parsed.as_str());
    Ok(url)
}

fn read_body(data: Option<String>) -> Result<Option<String>, ApixError> {
    let Some(data) = data else {
        return Ok(None);
    };
    if data == "@-" {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        return Ok(Some(input));
    }
    if let Some(path) = data.strip_prefix('@') {
        return Ok(Some(std::fs::read_to_string(path)?));
    }
    Ok(Some(data))
}

fn resolve_route_file(
    route: &str,
    source_override: Option<&str>,
) -> Result<(PathBuf, HashMap<String, String>, String), ApixError> {
    if let Ok(direct) = resolver::resolve_route_path(route, source_override) {
        return Ok((direct.file_path, HashMap::new(), direct.source));
    }

    let (sources, namespace, version, parts) =
        resolver::route_resolution_inputs(route, source_override)?;
    if parts.len() < 2 {
        return Err(ApixError::RouteNotFound(route.to_string()));
    }
    let method = parts
        .last()
        .ok_or_else(|| ApixError::RouteNotFound(route.to_string()))?
        .to_string();
    let path_parts = &parts[..parts.len() - 1];

    let mut matches: Vec<(PathBuf, HashMap<String, String>, String)> = Vec::new();
    for source in sources {
        let mut current = resolver::source_root(&source)?
            .join(&namespace)
            .join(&version);
        let mut captured = HashMap::new();
        let mut ok = true;

        for segment in path_parts {
            let exact = current.join(segment);
            if exact.is_dir() {
                current = exact;
                continue;
            }
            match find_param_dir(&current) {
                Ok((param_key, param_dir)) => {
                    captured.insert(param_key, segment.to_string());
                    current = param_dir;
                }
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }

        let route_file = current.join(format!("{method}.md"));
        if route_file.exists() {
            matches.push((route_file, captured, source));
        }
    }

    match matches.len() {
        0 => Err(ApixError::RouteNotFound(route.to_string())),
        1 => Ok(matches.remove(0)),
        _ => Err(ApixError::Ambiguous(format!(
            "Route `{route}` exists in multiple sources. Use --source or source-prefixed route."
        ))),
    }
}

fn find_param_dir(base: &Path) -> Result<(String, PathBuf), ApixError> {
    let entries = std::fs::read_dir(base)?;
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('{') && name.ends_with('}') && name.len() > 2 {
            let key = name
                .trim_start_matches('{')
                .trim_end_matches('}')
                .to_string();
            return Ok((key, entry.path()));
        }
    }
    Err(ApixError::RouteNotFound(format!(
        "No matching path segment under {}",
        base.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::set_var;
    use mockito::{Matcher, Server};
    use serial_test::serial;

    #[test]
    fn parses_header_key_value() {
        let (k, v) = split_header("Authorization: Bearer token").expect("header");
        assert_eq!(k, "Authorization");
        assert_eq!(v, "Bearer token");
    }

    #[test]
    fn applies_query_pairs_to_url() {
        let out = apply_query(
            "https://example.com/items".to_string(),
            &["a=1".to_string(), "b=x".to_string()],
        )
        .expect("query");
        assert!(out.contains("a=1"));
        assert!(out.contains("b=x"));
    }

    #[test]
    #[serial]
    fn resolves_literal_segment_to_param_directory() {
        let home = std::env::temp_dir().join(format!("apix-exec-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("vaults/.local/demo/v1/items/{id}")).expect("mkdir");
        std::fs::write(
            home.join("vaults/.local/demo/v1/items/{id}/GET.md"),
            "---\nmethod: GET\nurl: https://example.com/items/{id}\nauth: null\ncontent_type: application/json\n---\n",
        )
        .expect("write");

        set_var("APIX_HOME", &home);
        let (path, params, source) =
            resolve_route_file("demo/v1/items/item_123/GET", None).expect("resolve");
        assert_eq!(source, ".local");
        assert!(
            path.to_string_lossy()
                .ends_with("/vaults/.local/demo/v1/items/{id}/GET.md")
        );
        assert_eq!(params.get("id").map(String::as_str), Some("item_123"));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    #[serial]
    fn call_hits_mock_server_with_path_and_query_injection() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/items/item_123")
            .match_query(Matcher::UrlEncoded("expand".into(), "full".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .create();

        let home = std::env::temp_dir().join(format!("apix-exec-call-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("vaults/.local/demo/v1/items/{id}")).expect("mkdir");
        std::fs::write(
            home.join("vaults/.local/demo/v1/items/{id}/GET.md"),
            format!(
                "---\nmethod: GET\nurl: {}/items/{{id}}\nauth: null\ncontent_type: application/json\n---\n# Get item\n",
                server.url()
            ),
        )
        .expect("write route");

        set_var("APIX_HOME", &home);
        let result = call(
            "demo/v1/items/item_123/GET".to_string(),
            vec![],
            None,
            vec![],
            vec!["expand=full".to_string()],
            false,
            false,
            None,
        );
        assert!(result.is_ok());
        mock.assert();

        let _ = std::fs::remove_dir_all(&home);
    }
}
