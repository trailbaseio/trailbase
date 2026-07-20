#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
DEV_TOOLS_DIR="${ROOT_DIR}/.dev-tools"
DEB_CACHE_DIR="${DEV_TOOLS_DIR}/debs"

if ! command -v apt-get >/dev/null 2>&1 || ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "This bootstrap script currently supports Debian/Ubuntu-style systems with apt-get and dpkg-deb." >&2
  exit 1
fi

mkdir -p "${DEB_CACHE_DIR}"

download_and_extract() {
  local target_dir="$1"
  shift

  mkdir -p "${target_dir}"
  (
    cd "${DEB_CACHE_DIR}"
    apt-get download "$@"
    for package in "$@"; do
      for deb in "${package}"_*.deb; do
        dpkg-deb -x "${deb}" "${target_dir}"
      done
    done
  )
}

download_and_extract \
  "${DEV_TOOLS_DIR}/libclang-18" \
  libclang-18-dev \
  libclang1-18 \
  libclang-common-18-dev \
  clang-18

download_and_extract \
  "${DEV_TOOLS_DIR}/geos" \
  libgeos-dev \
  libgeos-c1t64 \
  libgeos3.12.1t64

download_and_extract \
  "${DEV_TOOLS_DIR}/protobuf" \
  protobuf-compiler \
  libprotobuf32t64 \
  libprotoc32t64 \
  libprotobuf-dev

if command -v corepack >/dev/null 2>&1; then
  corepack enable pnpm
fi

if command -v pnpm >/dev/null 2>&1; then
  (
    cd "${ROOT_DIR}"
    pnpm install --prefer-frozen-lockfile
  )
else
  echo "pnpm not found. Install pnpm or enable it through corepack before running cargo." >&2
fi

echo "Local dev tools bootstrapped in ${DEV_TOOLS_DIR}."
