# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.6](https://github.com/apix-sh/cli/compare/v0.1.5...v0.1.6) - 2026-03-05

### Added

- Improve registry module output by wrapping and indenting descriptions and tags using the new `textwrap` dependency.
- Extract and index API tags from OpenAPI specifications, including them in metadata and registry entries.
- Implement a utility to read HTTP response bodies with an increased 500MB safety limit, replacing direct `ureq::Response::into_string` calls.

### Fixed

- Clamp out-of-range integers in OpenAPI specs to prevent deserialization errors.

## [0.1.5](https://github.com/apix-sh/cli/compare/v0.1.4...v0.1.5) - 2026-03-05

### Added

- Parse and display OpenAPI security scheme information in generated routes and templates.
- Add `apix info` command to display API metadata and introduce schema reference resolution.
- Enable filtering `apix ls` output by a path prefix for specific namespace versions.

### Other

- update README

## [0.1.4](https://github.com/apix-sh/cli/compare/v0.1.3...v0.1.4) - 2026-03-04

### Added

- Add a GitHub Actions job to update the `public/install.sh` version in the `apix-sh/web` repository with the new release tag.
- customize markdown output styling and display frontmatter as formatted tables.

### Other

- Reordered Homebrew installation instructions in README.

## [0.1.3](https://github.com/apix-sh/cli/compare/v0.1.2...v0.1.3) - 2026-03-04

### Added

- Introduce and apply colored output formatting for sources, namespaces, methods, paths, and line numbers across CLI commands.
- allow apix pull to accept a specific namespace version ([#4](https://github.com/apix-sh/cli/pull/4))
- update apix search output format to group by source ([#5](https://github.com/apix-sh/cli/pull/5))

### Other

- Improve template conditional rendering, whitespace control, and variable quoting for generated markdown.
- Rename Homebrew tap from `apix-sh/apix` to `apix-sh/tap` and update all references in documentation and the release workflow.
- improve test coverage across multiple modules ([#6](https://github.com/apix-sh/cli/pull/6))

## [0.1.2](https://github.com/apix-sh/cli/compare/v0.1.1...v0.1.2) - 2026-03-02

### Fixed

- enhance route documentation to include response headers, content schemas
- Add support for header and cookie parameters and enhance request body examples with serialization hints and varied content types.
- Introduce a resolver module to handle OpenAPI `$ref` references for path items and schemas during build.

## [0.1.1](https://github.com/apix-sh/cli/compare/v0.1.0...v0.1.1) - 2026-03-01

### Fixed

- Generate relative Markdown links for type references instead of `apix peek` commands in generated documentation.
- table formatting issue in generated md files

### Other

- add SKILL.md
- add ascii banner
- relocate install script and update README
- adjust release-plz workflow.
- release v0.1.0 ([#1](https://github.com/apix-sh/cli/pull/1))

## [0.1.0](https://github.com/apix-sh/cli/releases/tag/v0.1.0) - 2026-03-01

### Other

- update cargo.lock
- Rename package to `apix-cli` and explicitly define `apix` as the binary target.
- update Cargo.toml
- Initial commit
