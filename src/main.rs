mod build;
mod cli;
mod config;
mod error;
mod exec;
mod http;
mod inventory;
mod output;
mod registry;
mod search;
#[cfg(test)]
mod test_env;
mod vault;

use clap::Parser;
use cli::{Cli, Commands, RegistryCommands, SourceCommands};
use error::ApixError;

fn run() -> Result<(), ApixError> {
    let cli = Cli::parse();
    output::set_options(output::OutputOptions {
        raw: cli.raw,
        no_color: cli.no_color,
        no_pager: cli.no_pager,
        json: cli.json,
        quiet: cli.quiet,
    });

    if !matches!(cli.command, Commands::Init | Commands::Completions { .. }) {
        let home = config::Config::apix_home()?;
        if !home.exists() {
            output::eprintln_info("Initializing ~/.apix for first use");
            config::init()?;
        }
    }

    config::migrate_legacy_local_layout()?;

    match cli.command {
        Commands::Search {
            query,
            source,
            all_sources,
            no_auto_update,
        } => registry::search(&query, source.as_deref(), all_sources, no_auto_update),
        Commands::Pull { namespace, source } => registry::pull(&namespace, source.as_deref()),
        Commands::Update {
            source,
            all_sources,
        } => registry::update(source.as_deref(), all_sources),
        Commands::Grep {
            namespace,
            query,
            limit,
            source,
        } => search::grep(&namespace, &query, limit, source.as_deref()),
        Commands::Peek { route, source } => vault::peek(&route, source.as_deref()),
        Commands::Info { target, source } => vault::info(&target, source.as_deref()),
        Commands::Show { route, source } => vault::show(&route, source.as_deref()),
        Commands::Call {
            route,
            headers,
            data,
            param,
            query,
            verbose,
            dry_run,
            source,
        } => exec::call(
            route,
            headers,
            data,
            param,
            query,
            verbose,
            dry_run,
            source.as_deref(),
        ),
        Commands::Import {
            source,
            name,
            output,
            overwrite,
        } => build::import(&source, &name, output.as_deref(), overwrite),
        Commands::Completions { shell } => cli::print_completions(shell),
        Commands::Init => config::init(),
        Commands::Source { command } => match command {
            SourceCommands::Add { name, remote } => registry::source_add(&name, &remote),
            SourceCommands::Remove { name } => registry::source_remove(&name),
            SourceCommands::List => registry::source_list(),
        },
        Commands::Registry { command } => match command {
            RegistryCommands::Rebuild { source, path } => {
                registry::rebuild(source.as_deref(), path.as_deref())
            }
        },
        Commands::Ls { namespace, source } => {
            inventory::ls(namespace.as_deref(), source.as_deref())
        }
    }
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(err) => {
            output::eprintln_error(&format!("{err}"));
            if let Some(hint) = actionable_hint(&err) {
                output::eprintln_warn(&format!("Hint: {hint}"));
            }
            match err {
                ApixError::Config(_)
                | ApixError::VaultNotFound(_)
                | ApixError::RouteNotFound(_)
                | ApixError::Ambiguous(_) => std::process::exit(1),
                _ => std::process::exit(2),
            }
        }
    }
}

fn actionable_hint(err: &ApixError) -> Option<&'static str> {
    match err {
        ApixError::VaultNotFound(msg) => {
            if msg.contains("registry.json") {
                Some(
                    "Run `apix update` (or `apix update --source <name>`) to fetch source metadata.",
                )
            } else {
                Some("Run `apix update` then `apix pull <namespace>`.")
            }
        }
        ApixError::RouteNotFound(_) => {
            Some("Use `apix grep <namespace> <query>` to discover available routes.")
        }
        ApixError::Parse(_) => {
            Some("Check command flags and input file format (OpenAPI 3.0/3.1 JSON or YAML).")
        }
        ApixError::Http(_) => Some("Verify network access, URL, headers, and required parameters."),
        ApixError::Git(_) => {
            Some("Ensure git is installed and retry `apix update` or `apix pull`.")
        }
        ApixError::Config(_) => {
            Some("Run `apix init` to create default config, or verify APIX_HOME/config.toml.")
        }
        ApixError::Ambiguous(_) => {
            Some("Disambiguate with --source or with a source-prefixed route.")
        }
        ApixError::Io(_) => Some("Verify filesystem paths and permissions."),
    }
}
