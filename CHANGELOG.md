# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
