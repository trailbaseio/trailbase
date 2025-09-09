TRAILBIN ?= RUST_BACKTRACE=1 cargo run --
TRAILDEPOT := traildepot

APPDIR := .
DISTDIR := ${APPDIR}/dist
ADDRESS := 127.0.0.1:4000

run: assets guest
	${TRAILBIN} --data-dir=${TRAILDEPOT} run --public-dir=${DISTDIR}/client --runtime-root-fs=. --address=${ADDRESS}

guest: ${TRAILDEPOT}/wasm/collab_clicker_ssr_rust_guest.wasm
#guest: ${TRAILDEPOT}/wasm/collab_clicker_ssr_ts_guest.wasm
assets: ${DISTDIR}/server/entry-server.js

${DISTDIR}/server/entry-server.js: $(shell find ${APPDIR}/src/ -type f)
	pnpm run build

${TRAILDEPOT}/wasm/collab_clicker_ssr_rust_guest.wasm: guests/rust/src/*.rs
	cargo build -p collab-clicker-ssr-guest --target wasm32-wasip2  --release \
		&& mkdir -p ${TRAILDEPOT}/wasm \
		&& rm -rf ${TRAILDEPOT}/wasm/* \
		&& cp ../../target/wasm32-wasip2/release/collab_clicker_ssr_guest.wasm $@

${TRAILDEPOT}/wasm/collab_clicker_ssr_ts_guest.wasm: guests/typescript/src/*.ts
	cd guests/typescript \
		&& pnpm install \
		&& pnpm build \
		&& cd ../.. \
		&& mkdir -p ${TRAILDEPOT}/wasm \
		&& rm -rf ${TRAILDEPOT}/wasm/* \
		&& cp guests/typescript/dist/component.wasm $@

clean:
	rm -rf ${DISTDIR} ${TRAILDEPOT}/data ${TRAILDEPOT}/wasm

.PHONY: run clean guest assets
