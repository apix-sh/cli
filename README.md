# apix

`apix` is a Rust CLI for importing, browsing, searching, and calling API endpoint docs stored as local markdown vaults.

## Features

- Import vaults from OpenAPI 3.0/3.1 specs (`apix import`)
- Browse endpoint/type docs (`apix show`, `apix peek`)
- Search local vault content (`apix grep`)
- Execute HTTP calls from vault frontmatter (`apix call`)
- Sync registry + namespaces with sparse-checkout (`apix update`, `apix pull`)
- Shell completions (`apix completions`)
- Auto-initializes `~/.apix` on first use

## Install

### Homebrew (recommended)

```bash
brew tap apix-sh/tap
brew install apix
```

Explicit form:

```bash
brew install apix-sh/tap/apix
```

### curl | sh

> Install script is maintained in the [web repo](https://github.com/apix-sh/web/blob/main/public/install.sh).

Latest:

```bash
curl -fsSL https://apix.sh/install | sh
```

Pinned version:

```bash
curl -fsSL https://apix.sh/install | sh -s -- --version v0.1.0
```

Custom install directory:

```bash
curl -fsSL https://apix.sh/install | sh -s -- --bin-dir "$HOME/.local/bin"
```

### Build from source

Prerequisites:

- Rust toolchain (`rustup`, `cargo`, `rustc`)

Build:

```bash
cargo build --release
```

Binary path:

```bash
./target/release/apix
```

### Verify install

```bash
apix --version
apix --help
```

## Quick Start

```bash
# Show help
cargo run -- --help

# Initialize config/home explicitly (optional)
cargo run -- init

# Import a local vault from an OpenAPI spec
cargo run -- import tests/fixtures/petstore.json --name petstore

# Import directly into a vault repo worktree
cargo run -- import tests/fixtures/petstore.json --name petstore --output /path/to/vault

# Show full route markdown
cargo run -- show petstore/v1/pets/GET

# Show condensed view (frontmatter + required request fields)
cargo run -- peek petstore/v1/pets/{petId}/GET

# Full-text grep within a namespace
cargo run -- grep petstore pet --limit 20 --source .local

# Indexed search across sources
cargo run -- search pet

# List local inventory
cargo run -- ls
cargo run -- ls --source .local
cargo run -- ls petstore

# List detailed routes for a namespace/version
cargo run -- ls petstore/v1
cargo run -- ls petstore/v1 --source core
```

## Command Reference

- `apix search <query> [--source <name>] [--all-sources] [--no-auto-update]`: Search indexed APIs
- `apix update [--source <name>] [--all-sources]`: Clone/pull source registry metadata
- `apix pull <namespace>[/<version>] [--source <name>]`: Sparse-checkout a namespace (or specific version) from a source (default: `core`)
- `apix import <source> --name <namespace> [--output <vault_root>] [--overwrite]`: Generate vault files from an OpenAPI spec
- `apix ls [namespace|namespace/version] [--source <name>]`: List local inventory or detailed routes
- `apix show <route> [--source <name>]`: Print full markdown for a route/type file
- `apix peek <route> [--source <name>]`: Print frontmatter + condensed required input info
- `apix grep <namespace> <query> [--limit N] [--source <name>]`: Search local markdown files
- `apix call <route> ... [--source <name>]`: Execute HTTP request resolved from route frontmatter
- `apix completions <bash|zsh|fish|elvish|powershell>`: Generate shell completions
- `apix init`: Create `~/.apix` structure and default config
- `apix source add/remove/list`: Manage third-party sources
- `apix registry rebuild [--source <name>] [--path <vault_root>]`: Rebuild registry index from a source root or vault repo path

## Route Format

Route strings are slash-separated.

Short form:

```text
<namespace>/<version>/<path segments>/<METHOD>
```

Explicit source form:

```text
<source>/<namespace>/<version>/<path segments>/<METHOD>
```

Example:

```text
petstore/v1/pets/{petId}/GET
core/petstore/v1/pets/{petId}/GET
```

## `apix call` Examples

```bash
# Route with literal path segment auto-mapped to {id}
cargo run -- call demo/v1/items/item_123/GET

# Explicit path/query/header/body flags
cargo run -- call demo/v1/items/{id}/POST \
  -p id=item_123 \
  -q expand=full \
  -H "Authorization: Bearer <token>" \
  -d '{"name":"item"}'

# Body from stdin
echo '{"name":"item"}' | cargo run -- call demo/v1/items/{id}/POST -p id=item_123 -d @-

# Body from file
cargo run -- call demo/v1/items/{id}/POST -p id=item_123 -d @payload.json
```

## Configuration

Default home directory:

- `~/.apix`
- override with `APIX_HOME`

Default registry remote:

- `https://github.com/apix-sh/vault.git`
- override with `APIX_REGISTRY_URL`

Auto-update controls:

- `auto_update = true|false` (default: `true`)
- `auto_update_ttl_seconds = 21600` (6 hours, `0` disables time-based auto-update checks)
- `APIX_AUTO_UPDATE` overrides `auto_update`
- `APIX_AUTO_UPDATE_TTL_SECONDS` overrides `auto_update_ttl_seconds`

Default source priority:

- `.local`, `core`
- override with `APIX_SOURCES` (comma-separated), e.g. `APIX_SOURCES=.local,core,acme`

Default config file:

```toml
color = true
pager = ""
auto_update = true
auto_update_ttl_seconds = 21600
sources = [".local", "core"]

[registry]
remote = "https://github.com/apix-sh/vault.git"

[source.acme]
remote = "https://github.com/acme/apix-vaults.git"
```

## Source Model

- Local builds are stored at `~/.apix/vaults/.local/<namespace>/<version>/...`
- Core registry is stored at `~/.apix/vaults/core/<namespace>/<version>/...`
- Third-party sources are stored at `~/.apix/vaults/<source>/<namespace>/<version>/...`

Short route resolution order follows `sources` config (or `APIX_SOURCES`).
If a short route exists in multiple sources, `apix` returns an ambiguity error and asks for `--source` or explicit source-prefixed route.

## Search vs Grep

- `search`: Indexed API discovery across sources using `registry.json` (can include not-yet-pulled APIs)
- `grep`: Full-text markdown search within one namespace/source (content-level search)
- `ls`: Local inventory and route listing from local files

`ls` modes:

- `apix ls`: list namespaces grouped by source
- `apix ls <namespace>`: list versions for a namespace across sources
- `apix ls <namespace>/<version>`: list paths and methods with one-line route summaries
- Route-detail mode resolves from one source only: `--source` if provided, else first match by priority (`.local` -> `core` -> third-party)

Registry lifecycle:

- `core`/third-party sources: registry index comes from source root (`registry.json`) after `update`/`pull`
- `.local` source: registry index is auto-rebuilt after `import`
- Manual reindex:
  - local source: `apix registry rebuild --source .local`
  - vault repo path: `apix registry rebuild --path /path/to/vault`

Vault repo spec (for `--path` mode and source compatibility):

- [`docs/vault-repo-spec.md`](docs/vault-repo-spec.md)

Search auto-update:

- `search` can auto-refresh source `registry.json` before matching
- Auto-update uses registry-only fetch; it does not update pulled namespace content
- `.local` is excluded from auto-update and timestamp tracking
- Last successful source refresh timestamp: `~/.apix/vaults/<source>/.last-updated`
- Skip once with `apix search ... --no-auto-update`

## Output Behavior

- If stdout is a TTY: markdown is rendered for humans (`termimad`)
- If stdout is piped: raw markdown is emitted
- `--raw` forces raw markdown even in TTY mode
- `--no-color` or `NO_COLOR=1` disables color styling
- `show` and `ls <namespace>/<version>` auto-page output in TTY mode using `pager` config / `PAGER` (fallback: `less -FRX`)
- `--no-pager` disables pager usage

## Development

```bash
cargo fmt --all
cargo test --locked
cargo check --locked
```

## Release Automation

Releases are automated with GitHub Actions + `release-plz`:

- `.github/workflows/release-plz.yml`:
  - opens/updates release PRs from commits on `main`
  - bumps `Cargo.toml` version and changelog
  - publishes crate + creates GitHub release after release PR merge
- `.github/workflows/release.yml`:
  - builds multi-platform binaries
  - uploads release tarballs and `SHA256SUMS` to the published GitHub release

Recommended commit style for clean changelogs:

- Conventional Commits (`feat:`, `fix:`, `feat!:`)
