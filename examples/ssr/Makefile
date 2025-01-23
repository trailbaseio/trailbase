TRAILBIN ?= RUST_BACKTRACE=1 cargo run --
DISTDIR := solid-ssr-app/dist

run: traildepot/scripts/entry-server.js
	${TRAILBIN} --data-dir=traildepot run --public-dir=${DISTDIR}/client

traildepot/scripts/entry-server.js: ${DISTDIR}/server/entry-server.js
	cp $< $@

${DISTDIR}/server/entry-server.js:
	cd solid-ssr-app; pnpm run build

clean:
	rm -rf solid-ssr-app/dist

.PHONY: run clean
