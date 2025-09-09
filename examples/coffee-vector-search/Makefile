TRAILBIN ?= RUST_BACKTRACE=1 cargo run --
TRAILDEPOT := traildepot

APPDIR := .
DISTDIR := ${APPDIR}/dist
ADDRESS := 127.0.0.1:4000

run: init assets guest
	${TRAILBIN} --data-dir=${TRAILDEPOT} run --public-dir=${DISTDIR} --address=${ADDRESS}

guest: ${TRAILDEPOT}/wasm/coffee_vector_search_rust_guest.wasm
#guest: ${TRAILDEPOT}/wasm/coffee_vector_search_ts_guest.wasm

assets:
	rm -rf dist && pnpm build

init:
	rm -rf traildepot/data && mkdir -p traildepot/data && cat import.sql | sqlite3 traildepot/data/main.db -

${TRAILDEPOT}/wasm/coffee_vector_search_rust_guest.wasm: guests/rust/src/*.rs
	cargo build -p coffee-vector-search-guest --target wasm32-wasip2  --release \
		&& mkdir -p ${TRAILDEPOT}/wasm \
		&& rm -rf ${TRAILDEPOT}/wasm/* \
		&& cp ../../target/wasm32-wasip2/release/coffee_vector_search_guest.wasm $@

${TRAILDEPOT}/wasm/coffee_vector_search_ts_guest.wasm: guests/typescript/src/*.ts
	cd guests/typescript \
		&& pnpm install \
		&& pnpm build \
		&& cd ../.. \
		&& mkdir -p ${TRAILDEPOT}/wasm \
		&& rm -rf ${TRAILDEPOT}/wasm/* \
		&& cp guests/typescript/dist/component.wasm $@

clean:
	rm -rf dist traildepot/data ${TRAILDEPOT}/wasm/*

.PHONY: init assets guest run clean
