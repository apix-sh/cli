# Vault Repository Spec v1

This document defines the expected structure for any vault repository consumed by `apix` sources.

## 1. Scope

A vault repository is a filesystem tree that contains:

- content files for namespaces and versions
- a root `registry.json` index for API-level discovery
- optional metadata and governance files

## 2. Required Layout

Repository root must contain:

- `registry.json`
- namespace directories at root level

Repository root may also include:

- `README.md`
- `CONTRIBUTION.md`

Namespace/version content layout:

```text
<repo-root>/
├── registry.json
├── <namespace>/
│   └── <version>/
│       ├── _metadata.md
│       ├── _types/
│       │   └── *.md
│       └── <route path>/
│           └── <METHOD>.md
```

Rules:

- Namespace directories are direct children of repo root.
- Version directories are direct children of namespace directories.
- Route files end with `.md` and use method filenames (`GET.md`, `POST.md`, etc.).
- `_metadata.md` is required for a valid `<namespace>/<version>` entry.

## 3. Optional Metadata Layout

Optional repository metadata should be placed under `.metadata/`:

```text
<repo-root>/
└── .metadata/
    ├── catalog.yaml
    ├── sources/
    ├── policies/
    └── scripts/
```

This keeps namespace root clean and avoids collisions with namespace names.

## 4. Namespace Scanning Rules

When rebuilding `registry.json`, scanners must ignore non-namespace directories/files:

- all dot-prefixed entries (for example `.metadata`, `.git`, `.github`)
- `registry.json`
- root files other than `registry.json` (for example `README.md`, `CONTRIBUTION.md`)
- root directories other than `.metadata` that do not contain valid namespace/version trees
- temporary/system artifacts

Only directories that contain valid version trees with `_metadata.md` participate in registry indexing.

Import/scanning behavior at repository root:

- `registry.json` is processed as index metadata.
- `.metadata/` is reserved for repository metadata and ignored as namespace content.
- Other root-level files and non-namespace directories are ignored.

## 5. Registry Expectations

`registry.json` should contain API-level discovery entries with at least:

- `name`
- `description`
- `versions`
- `tags`

For multi-source local resolution, source attribution can be inferred from source location.

## 6. Official Vault Notes

The official vault repository may include additional maintainer assets (scripts, policies, curation files), but must preserve:

- root `registry.json`
- flat-root namespace/version content layout

## 7. Compatibility

This spec aligns with current `apix` source model where source roots map to:

- `~/.apix/vaults/<source>/registry.json`
- `~/.apix/vaults/<source>/<namespace>/<version>/...`

Any repository following this spec can be consumed as a source with standard `update`, `pull`, and `search` behavior.
