use colored::Colorize;
use std::io::IsTerminal;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, Default)]
pub struct OutputOptions {
    pub raw: bool,
    pub no_color: bool,
    pub no_pager: bool,
    pub json: bool,
    pub quiet: bool,
}

static OUTPUT_OPTIONS: OnceLock<OutputOptions> = OnceLock::new();

pub fn set_options(opts: OutputOptions) {
    let _ = OUTPUT_OPTIONS.set(opts);
}

pub fn options() -> OutputOptions {
    OUTPUT_OPTIONS.get().copied().unwrap_or_default()
}

pub fn print_markdown(markdown: &str) {
    print!("{}", render_markdown(markdown));
}

pub fn print_markdown_with_optional_pager(markdown: &str) {
    let rendered = render_markdown(markdown);
    if should_use_pager(&rendered) && try_page(&rendered) {
        return;
    }
    print!("{rendered}");
}

pub fn eprintln_error(msg: &str) {
    if colors_disabled() {
        eprintln!("{msg}");
    } else {
        eprintln!("{}", msg.red().bold());
    }
}

pub fn eprintln_warn(msg: &str) {
    if options().quiet {
        return;
    }
    if colors_disabled() {
        eprintln!("{msg}");
    } else {
        eprintln!("{}", msg.yellow());
    }
}

pub fn eprintln_info(msg: &str) {
    if !options().quiet {
        eprintln!("{msg}");
    }
}

pub fn print_with_optional_pager(text: &str) {
    if should_use_pager(text) && try_page(text) {
        return;
    }
    print!("{text}");
}

fn render_markdown(markdown: &str) -> String {
    if should_print_raw() {
        return markdown.to_string();
    }

    let mut out = Vec::new();
    let write_res = if colors_disabled() {
        let skin = termimad::MadSkin::no_style();
        skin.write_text_on(&mut out, markdown)
    } else {
        let skin = termimad::MadSkin::default();
        skin.write_text_on(&mut out, markdown)
    };
    if write_res.is_err() {
        return markdown.to_string();
    }
    String::from_utf8(out).unwrap_or_else(|_| markdown.to_string())
}

fn should_print_raw() -> bool {
    let opts = options();
    opts.raw || !std::io::stdout().is_terminal()
}

fn colors_disabled() -> bool {
    let opts = options();
    opts.no_color || std::env::var_os("NO_COLOR").is_some() || !std::io::stdout().is_terminal()
}

fn should_use_pager(text: &str) -> bool {
    if options().no_pager || options().json || !std::io::stdout().is_terminal() {
        return false;
    }

    match terminal_height_lines() {
        Some(lines) if lines > 1 => text.lines().count() > lines.saturating_sub(1),
        _ => true,
    }
}

fn terminal_height_lines() -> Option<usize> {
    std::env::var("LINES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
}

fn try_page(text: &str) -> bool {
    let pager = configured_pager();
    let mut parts = pager.split_whitespace();
    let program = match parts.next() {
        Some(p) if !p.is_empty() => p,
        _ => return false,
    };
    let args: Vec<&str> = parts.collect();

    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(stdin) = child.stdin.as_mut() {
        if stdin.write_all(text.as_bytes()).is_err() {
            let _ = child.kill();
            return false;
        }
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn configured_pager() -> String {
    if let Ok(cfg) = crate::config::Config::load() {
        if let Some(p) = cfg.pager {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    if let Ok(pager) = std::env::var("PAGER") {
        let trimmed = pager.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    // -F: quit if content fits on one screen; -R: keep ANSI colors; -X: keep screen content
    "less -FRX".to_string()
}
