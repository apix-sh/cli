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

pub fn eprint_temp(msg: &str) {
    if options().quiet {
        return;
    }
    let mut stderr = std::io::stderr();
    let _ = termimad::crossterm::ExecutableCommand::execute(&mut stderr, termimad::crossterm::terminal::Clear(termimad::crossterm::terminal::ClearType::CurrentLine));
    let _ = termimad::crossterm::ExecutableCommand::execute(&mut stderr, termimad::crossterm::cursor::MoveToColumn(0));
    let _ = write!(stderr, "{}", msg.dimmed());
    let _ = stderr.flush();
}

pub fn clear_temp() {
    if options().quiet {
        return;
    }
    let mut stderr = std::io::stderr();
    let _ = termimad::crossterm::ExecutableCommand::execute(&mut stderr, termimad::crossterm::terminal::Clear(termimad::crossterm::terminal::ClearType::CurrentLine));
    let _ = termimad::crossterm::ExecutableCommand::execute(&mut stderr, termimad::crossterm::cursor::MoveToColumn(0));
    let _ = stderr.flush();
}

pub fn print_with_optional_pager(text: &str) {
    if should_use_pager(text) && try_page(text) {
        return;
    }
    print!("{text}");
}

static CUSTOM_TABLE_BORDERS: termimad::TableBorderChars = termimad::TableBorderChars {
    horizontal: '─',
    vertical: ' ',
    top_left_corner: ' ',
    top_right_corner: ' ',
    bottom_right_corner: ' ',
    bottom_left_corner: ' ',
    top_junction: '─',
    right_junction: '─',
    bottom_junction: '─',
    left_junction: '─',
    cross: '─',
};

fn make_skin() -> termimad::MadSkin {
    use termimad::crossterm::style::{Attribute, Color};

    let mut skin = if colors_disabled() {
        termimad::MadSkin::no_style()
    } else {
        let mut s = termimad::MadSkin::default();

        // h1: underlined, bold, green (AnsiValue(10))
        s.headers[0].compound_style.set_fg(Color::AnsiValue(10));
        s.headers[0].compound_style.add_attr(Attribute::Bold);
        s.headers[0].compound_style.add_attr(Attribute::Underlined);

        // h2: bold green (AnsiValue(10)), NOT underlined
        s.headers[1].compound_style.set_fg(Color::AnsiValue(10));
        s.headers[1].compound_style.add_attr(Attribute::Bold);
        s.headers[1].compound_style.remove_attr(Attribute::Underlined);

        // h3: green (AnsiValue(10)), NOT underlined
        s.headers[2].compound_style.set_fg(Color::AnsiValue(10));
        s.headers[2].compound_style.remove_attr(Attribute::Underlined);

        // h4: bold white (AnsiValue(15)), NOT underlined
        s.headers[3].compound_style.set_fg(Color::AnsiValue(15));
        s.headers[3].compound_style.add_attr(Attribute::Bold);
        s.headers[3].compound_style.remove_attr(Attribute::Underlined);

        s
    };

    skin.headers[0].align = termimad::Alignment::Left;
    skin.headers[1].align = termimad::Alignment::Left;
    skin.headers[2].align = termimad::Alignment::Left;
    skin.headers[3].align = termimad::Alignment::Left;

    skin.inline_code.object_style.background_color = None;

    skin.table_border_chars = &CUSTOM_TABLE_BORDERS;

    skin
}

fn render_markdown(markdown: &str) -> String {
    if should_print_raw() {
        return markdown.to_string();
    }

    let mut out = Vec::new();
    let skin = make_skin();
    let write_res = skin.write_text_on(&mut out, markdown);
    
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

pub fn fmt_source(s: &str) -> String {
    if colors_disabled() {
        s.to_string()
    } else {
        s.bright_green().underline().to_string()
    }
}

pub fn fmt_namespace(s: &str) -> String {
    if colors_disabled() {
        s.to_string()
    } else {
        s.bold().to_string()
    }
}

pub fn fmt_method(s: &str) -> String {
    if colors_disabled() {
        s.to_string()
    } else {
        s.bold().to_string()
    }
}

pub fn fmt_path(s: &str) -> String {
    if colors_disabled() {
        s.to_string()
    } else {
        s.bright_green().underline().to_string()
    }
}

pub fn fmt_line_number(s: &str) -> String {
    if colors_disabled() {
        s.to_string()
    } else {
        s.bold().to_string()
    }
}

pub fn print_indented_dimmed(text: &str, indent_size: usize) {
    let width = termimad::terminal_size().0 as usize;
    let width = if width == 0 { 80 } else { width };
    let indent = " ".repeat(indent_size);
    let options = textwrap::Options::new(width.saturating_sub(indent_size))
        .initial_indent(&indent)
        .subsequent_indent(&indent);
    
    let wrapped = textwrap::fill(text, options);
    if colors_disabled() {
        println!("{wrapped}");
    } else {
        println!("{}", wrapped.dimmed());
    }
}

pub fn print_indented_dimmed_tags(text: &str, indent_size: usize) {
    let width = termimad::terminal_size().0 as usize;
    let width = if width == 0 { 80 } else { width };
    let indent = " ".repeat(indent_size);
    let options = textwrap::Options::new(width.saturating_sub(indent_size))
        .initial_indent(&indent)
        .subsequent_indent(&indent);
    
    let wrapped = textwrap::fill(text, options);
    if colors_disabled() {
        println!("{wrapped}");
    } else {
        println!("{}", wrapped.cyan().dimmed());
    }
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

    if let Some(stdin) = child.stdin.as_mut()
        && stdin.write_all(text.as_bytes()).is_err() {
            let _ = child.kill();
            return false;
        }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn configured_pager() -> String {
    if let Ok(cfg) = crate::config::Config::load()
        && let Some(p) = cfg.pager {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
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
