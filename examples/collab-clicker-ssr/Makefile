TRAILBIN ?= RUST_BACKTRACE=1 cargo run --
TRAILDEPOT := traildepot

APPDIR := .
DISTDIR := ${APPDIR}/dist
ADDRESS := 127.0.0.1:4000

run: assets guest
	${TRAILBIN} --data-dir=${TRAILDEPOT} run --public-dir=${DISTDIR}/client --wasm-root-dir=. --address=${ADDRESS}

guest: ${TRAILDEPOT}/wasm/collab_clicker_ssr_guest.wasm
assets: ${DISTDIR}/server/entry-server.js

${DISTDIR}/server/entry-server.js: $(shell find ${APPDIR}/src/ -type f)
	pnpm run build

${TRAILDEPOT}/wasm/collab_clicker_ssr_guest.wasm:
	cargo build -p collab-clicker-ssr-guest --target wasm32-wasip2  --release \
		&& mkdir -p ${TRAILDEPOT}/wasm \
		&& cp ../../target/wasm32-wasip2/release/collab_clicker_ssr_guest.wasm ${TRAILDEPOT}/wasm/

clean:
	rm -rf ${DISTDIR} ${TRAILDEPOT}/data ${TRAILDEPOT}/wasm

.PHONY: run clean guest assets
