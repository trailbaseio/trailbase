TRAILBIN ?= RUST_BACKTRACE=1 cargo run --
TRAILDEPOT := traildepot

APPDIR := .
DISTDIR := ${APPDIR}/dist
ADDRESS := 127.0.0.1:4000

run: ${TRAILDEPOT}/scripts/entry-server.js
	${TRAILBIN} --data-dir=${TRAILDEPOT} run --public-dir=${DISTDIR}/client --address=${ADDRESS}

${TRAILDEPOT}/scripts/entry-server.js: ${DISTDIR}/server/entry-server.js
	cp $< $@

${DISTDIR}/server/entry-server.js: $(shell find ${APPDIR}/src/ -type f)
	pnpm run build

clean:
	rm -rf ${DISTDIR} ${TRAILDEPOT}/data

.PHONY: run clean
