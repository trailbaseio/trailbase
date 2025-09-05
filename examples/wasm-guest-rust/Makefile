TRAILBIN ?= RUST_BACKTRACE=1 cargo run -p trailbase-cli --
TRAILDEPOT := traildepot

APPDIR := .
DISTDIR := ${APPDIR}/dist
ADDRESS := 127.0.0.1:4000

run: guest
	${TRAILBIN} --data-dir=${TRAILDEPOT} run --address=${ADDRESS}

guest: ${TRAILDEPOT}/wasm/component.wasm

${TRAILDEPOT}/wasm/component.wasm: ../../target/wasm32-wasip2/release/rust_guest.wasm
	mkdir -p ${TRAILDEPOT}/wasm && cp $< $@

../../target/wasm32-wasip2/release/rust_guest.wasm: src/*.rs
	cargo build --target wasm32-wasip2  --release

.PHONY: guest run
