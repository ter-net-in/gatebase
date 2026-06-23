#!/bin/sh
set -eu

repo="ter-net-in/gatebase"
bin="gatebase"
install_dir="${INSTALL_DIR:-/usr/local/bin}"

usage() {
  printf 'Usage: install.sh [--uninstall]\n'
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'error: %s is required\n' "$1" >&2
    exit 1
  fi
}

run_privileged() {
  if [ -w "$install_dir" ]; then
    "$@"
  else
    need sudo
    sudo "$@"
  fi
}

uninstall() {
  target="$install_dir/$bin"
  if [ ! -e "$target" ]; then
    printf '%s not found at %s\n' "$bin" "$target"
    exit 0
  fi
  run_privileged rm -f "$target"
  printf 'removed %s\n' "$target"
}

target_triple() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os:$arch" in
    Linux:x86_64|Linux:amd64) printf 'x86_64-unknown-linux-gnu' ;;
    Linux:aarch64|Linux:arm64) printf 'aarch64-unknown-linux-gnu' ;;
    Darwin:x86_64|Darwin:amd64) printf 'x86_64-apple-darwin' ;;
    Darwin:aarch64|Darwin:arm64) printf 'aarch64-apple-darwin' ;;
    *)
      printf 'error: unsupported platform %s/%s\n' "$os" "$arch" >&2
      exit 1
      ;;
  esac
}

latest_tag() {
  curl -fsSL "https://api.github.com/repos/$repo/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' \
    | head -n 1
}

install_gatebase() {
  need curl
  need tar
  need mktemp

  triple=$(target_triple)
  tag="${GATEBASE_VERSION:-$(latest_tag)}"
  if [ -z "$tag" ]; then
    printf 'error: could not resolve latest release\n' >&2
    exit 1
  fi
  version=${tag#v}
  archive="gatebase-$version-$triple.tar.gz"
  url="https://github.com/$repo/releases/download/v$version/$archive"
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT INT TERM

  printf 'downloading %s\n' "$url"
  curl -fL "$url" -o "$tmp/$archive"
  tar -xzf "$tmp/$archive" -C "$tmp"

  run_privileged mkdir -p "$install_dir"
  run_privileged install -m 0755 "$tmp/$bin" "$install_dir/$bin"
  printf 'installed %s\n' "$install_dir/$bin"
}

case "${1:-}" in
  --uninstall) uninstall ;;
  -h|--help) usage ;;
  "") install_gatebase ;;
  *) usage >&2; exit 1 ;;
esac
