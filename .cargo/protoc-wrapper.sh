#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "${SCRIPT_DIR}/.." && pwd)
PROTOBUF_DIR="${REPO_ROOT}/.dev-tools/protobuf"

export LD_LIBRARY_PATH="${PROTOBUF_DIR}/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
exec "${PROTOBUF_DIR}/usr/bin/protoc" \
  -I"${PROTOBUF_DIR}/usr/include" \
  "$@"
