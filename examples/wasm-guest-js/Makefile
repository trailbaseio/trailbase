TRAILBIN ?= RUST_BACKTRACE=1 cargo run --
TRAILDEPOT := traildepot

APPDIR := .
DISTDIR := ${APPDIR}/dist
ADDRESS := 127.0.0.1:4000

run: guest
	${TRAILBIN} --data-dir=${TRAILDEPOT} run --address=${ADDRESS}

guest: ${TRAILDEPOT}/wasm/component.wasm

${TRAILDEPOT}/wasm/component.wasm: ${DISTDIR}/component.wasm
	mkdir -p ${TRAILDEPOT}/wasm && cp $< $@

${DISTDIR}/component.wasm:
	pnpm i && pnpm build

clean:
	rm -rf dist traildepot

.PHONY: guest run clean
