TRAILBIN ?= RUST_BACKTRACE=1 cargo run -p trailbase-cli --
TRAILDEPOT := traildepot
NAME := auth_ui_component

APPDIR := .
DISTDIR := ${APPDIR}/dist
ADDRESS := 127.0.0.1:4000

deploy: ${TRAILDEPOT}/wasm/component.wasm
	cp $< ../../client/testfixture/wasm/auth_ui_component.wasm

run: ${TRAILDEPOT}/wasm/component.wasm
	${TRAILBIN} --data-dir=${TRAILDEPOT} run --address=${ADDRESS}

${TRAILDEPOT}/wasm/component.wasm: ../../target/wasm32-wasip2/release/${NAME}.wasm
	mkdir -p ${TRAILDEPOT}/wasm && cp $< $@

../../target/wasm32-wasip2/release/${NAME}.wasm: src/*.rs FORCE
	cargo build --target wasm32-wasip2  --release

FORCE:

.PHONY: deploy run FORCE
