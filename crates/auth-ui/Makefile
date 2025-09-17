TRAILBIN ?= RUST_BACKTRACE=1 cargo run -p trailbase-cli --
TRAILDEPOT := traildepot
NAME := auth_ui_component

APPDIR := .
DISTDIR := ${APPDIR}/dist
ADDRESS := 127.0.0.1:4000

guest: ${TRAILDEPOT}/wasm/component.wasm

deploy: ${TRAILDEPOT}/wasm/component.wasm
	cp $< ../../client/testfixture/wasm/auth_ui_component.wasm

run: guest
	${TRAILBIN} --data-dir=${TRAILDEPOT} run --address=${ADDRESS}

${TRAILDEPOT}/wasm/component.wasm: ../../target/wasm32-wasip2/release/${NAME}.wasm
	mkdir -p ${TRAILDEPOT}/wasm && cp $< $@

../../target/wasm32-wasip2/release/${NAME}.wasm: src/*.rs
	cargo build --target wasm32-wasip2  --release

.PHONY: guest run
