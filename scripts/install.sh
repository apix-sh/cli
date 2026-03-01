#!/bin/sh
set -eu

REPO="${APIX_INSTALL_REPO:-apix-sh/cli}"
BIN_NAME="apix"
BIN_DIR="${APIX_INSTALL_BIN_DIR:-$HOME/.local/bin}"
VERSION=""

usage() {
  cat <<EOF
apix installer

Usage:
  install.sh [--version <tag>] [--bin-dir <dir>] [--help]

Examples:
  curl -fsSL https://apix.sh/install | sh
  curl -fsSL https://apix.sh/install | sh -s -- --version v0.1.0
  curl -fsSL https://apix.sh/install | sh -s -- --bin-dir \$HOME/bin
EOF
}

log() {
  printf "%s\n" "$*"
}

die() {
  printf "error: %s\n" "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --version)
        [ "$#" -ge 2 ] || die "--version requires a value"
        VERSION="$2"
        shift 2
        ;;
      --bin-dir)
        [ "$#" -ge 2 ] || die "--bin-dir requires a value"
        BIN_DIR="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
  done
}

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *)
      die "unsupported OS: $os"
      ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *)
      die "unsupported architecture: $arch"
      ;;
  esac

  TARGET="${arch}-${os}"
}

fetch_latest_version() {
  need_cmd curl
  api="https://api.github.com/repos/${REPO}/releases/latest"
  VERSION="$(curl -fsSL "$api" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
  [ -n "$VERSION" ] || die "unable to determine latest release tag from ${api}"
}

download_file() {
  url="$1"
  out="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$out"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$out" "$url"
  else
    die "need curl or wget to download files"
  fi
}

sha256_file() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    die "need sha256sum or shasum for checksum verification"
  fi
}

install_binary() {
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT INT TERM

  archive="apix-${VERSION}-${TARGET}.tar.gz"
  base_url="https://github.com/${REPO}/releases/download/${VERSION}"
  archive_url="${base_url}/${archive}"
  sums_url="${base_url}/SHA256SUMS"

  log "Installing ${BIN_NAME} ${VERSION} (${TARGET})"
  log "Downloading ${archive_url}"

  download_file "$archive_url" "$tmp/$archive"
  download_file "$sums_url" "$tmp/SHA256SUMS"

  expected="$(grep " ${archive}\$" "$tmp/SHA256SUMS" | awk '{print $1}')"
  [ -n "$expected" ] || die "checksum for ${archive} not found in SHA256SUMS"
  actual="$(sha256_file "$tmp/$archive")"
  [ "$expected" = "$actual" ] || die "checksum mismatch for ${archive}"

  mkdir -p "$tmp/unpack"
  tar -xzf "$tmp/$archive" -C "$tmp/unpack"
  [ -f "$tmp/unpack/${BIN_NAME}" ] || die "binary ${BIN_NAME} not found in archive"

  mkdir -p "$BIN_DIR"
  install -m 0755 "$tmp/unpack/${BIN_NAME}" "$BIN_DIR/${BIN_NAME}"

  log "Installed ${BIN_NAME} to ${BIN_DIR}/${BIN_NAME}"
  case ":$PATH:" in
    *:"$BIN_DIR":*)
      ;;
    *)
      log "Add ${BIN_DIR} to PATH to run '${BIN_NAME}' directly."
      ;;
  esac
}

main() {
  parse_args "$@"
  detect_target

  if [ -z "$VERSION" ]; then
    fetch_latest_version
  fi

  install_binary
}

main "$@"
