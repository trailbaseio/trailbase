TRAILBIN ?= RUST_BACKTRACE=1 cargo run --
DISTDIR := dist

run: traildepot/scripts/entry-server.js
	${TRAILBIN} --data-dir=traildepot run --public-dir=${DISTDIR}/client

traildepot/scripts/entry-server.js: ${DISTDIR}/server/entry-server.js
	cp $< $@

${DISTDIR}/server/entry-server.js:
	pnpm run build

clean:
	rm -rf dist

.PHONY: run clean
