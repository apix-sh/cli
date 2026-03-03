use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Generator, Shell, generate};

#[derive(Debug, Clone, Parser)]
#[command(
    name = "apix",
    version,
    about = "API Explorer for Agents (and Humans)",
    long_about = "API Explorer for Agents (and Humans)\n\nLocal-first, progressive disclosure API discovery and browsing CLI for the agentic era.",
    before_help = r#"

 █████╗ ██████╗ ██╗██╗  ██╗
██╔══██╗██╔══██╗██║╚██╗██╔╝
███████║██████╔╝██║ ╚███╔╝
██╔══██║██╔═══╝ ██║ ██╔██╗
██║  ██║██║     ██║██╔╝ ██╗
╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝"#
)]
pub struct Cli {
    #[arg(long = "no-color", global = true)]
    pub no_color: bool,
    #[arg(long = "no-pager", global = true)]
    pub no_pager: bool,
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true)]
    pub quiet: bool,
    #[arg(long, global = true)]
    pub raw: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// Search the registry for APIs
    Search {
        query: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        all_sources: bool,
        #[arg(long)]
        no_auto_update: bool,
    },

    /// Pull an API vault (or specific version) from the registry
    Pull {
        #[arg(value_name = "NAMESPACE[/VERSION]")]
        namespace: String,
        #[arg(long)]
        source: Option<String>,
    },

    /// Update the local registry
    Update {
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        all_sources: bool,
    },

    /// Full-text search within a pulled vault
    Grep {
        namespace: String,
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        source: Option<String>,
    },

    /// Show frontmatter + required params only
    Peek {
        route: String,
        #[arg(long)]
        source: Option<String>,
    },

    /// Show full endpoint documentation
    Show {
        route: String,
        #[arg(long)]
        source: Option<String>,
    },

    /// Execute an API call
    Call {
        route: String,
        #[arg(short = 'H', long = "header", num_args = 1)]
        headers: Vec<String>,
        #[arg(short, long)]
        data: Option<String>,
        #[arg(short, long, num_args = 1)]
        param: Vec<String>,
        #[arg(short, long)]
        query: Vec<String>,
        #[arg(long)]
        verbose: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        source: Option<String>,
    },

    /// Import a vault from an OpenAPI spec
    Import {
        source: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        output: Option<String>,
        #[arg(long)]
        overwrite: bool,
    },

    /// Generate shell completion scripts
    Completions { shell: CompletionShell },

    /// Initialize ~/.apix
    Init,

    /// Manage sources
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },

    /// Manage local source registry indexes
    Registry {
        #[command(subcommand)]
        command: RegistryCommands,
    },

    /// List local inventory or routes for namespace/version
    Ls {
        namespace: Option<String>,
        #[arg(long)]
        source: Option<String>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SourceCommands {
    /// Add a source
    Add {
        name: String,
        #[arg(long)]
        remote: String,
    },
    /// Remove a source
    Remove { name: String },
    /// List configured sources
    List,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RegistryCommands {
    /// Rebuild a registry index from a source root or repository path
    Rebuild {
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        path: Option<String>,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    PowerShell,
    Zsh,
}

impl CompletionShell {
    fn as_clap_shell(&self) -> Shell {
        match self {
            Self::Bash => Shell::Bash,
            Self::Elvish => Shell::Elvish,
            Self::Fish => Shell::Fish,
            Self::PowerShell => Shell::PowerShell,
            Self::Zsh => Shell::Zsh,
        }
    }
}

pub fn print_completions(shell: CompletionShell) -> Result<(), crate::error::ApixError> {
    let mut cmd = Cli::command();
    print_for(shell.as_clap_shell(), &mut cmd);
    Ok(())
}

fn print_for<G: Generator>(generator: G, cmd: &mut clap::Command) {
    generate(
        generator,
        cmd,
        cmd.get_name().to_string(),
        &mut std::io::stdout(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_import_subcommand() {
        let cli = Cli::parse_from(["apix", "import", "spec.yaml", "--name", "demo"]);
        match cli.command {
            Commands::Import {
                source,
                name,
                output,
                overwrite,
            } => {
                assert_eq!(source, "spec.yaml");
                assert_eq!(name, "demo");
                assert!(output.is_none());
                assert!(!overwrite);
            }
            other => panic!("expected import, got {other:?}"),
        }
    }

    #[test]
    fn parses_import_output_and_overwrite_flags() {
        let cli = Cli::parse_from([
            "apix",
            "import",
            "spec.yaml",
            "--name",
            "demo",
            "--output",
            "/tmp/vault",
            "--overwrite",
        ]);
        match cli.command {
            Commands::Import {
                output, overwrite, ..
            } => {
                assert_eq!(output.as_deref(), Some("/tmp/vault"));
                assert!(overwrite);
            }
            other => panic!("expected import, got {other:?}"),
        }
    }

    #[test]
    fn rejects_build_subcommand() {
        let err = Cli::try_parse_from(["apix", "build", "spec.yaml", "--name", "demo"])
            .expect_err("build should be rejected pre-release");
        let msg = err.to_string();
        assert!(msg.contains("unrecognized subcommand"));
        assert!(msg.contains("build"));
    }

    #[test]
    fn test_print_completions() {
        // Just verify it doesn't panic
        let _ = print_completions(CompletionShell::Bash);
    }

    #[test]
    fn completion_shell_as_clap_shell() {
        assert_eq!(CompletionShell::Bash.as_clap_shell(), Shell::Bash);
        assert_eq!(CompletionShell::Elvish.as_clap_shell(), Shell::Elvish);
        assert_eq!(CompletionShell::Fish.as_clap_shell(), Shell::Fish);
        assert_eq!(
            CompletionShell::PowerShell.as_clap_shell(),
            Shell::PowerShell
        );
        assert_eq!(CompletionShell::Zsh.as_clap_shell(), Shell::Zsh);
    }
}
