all: init app

app: dist
	pnpm build

init:
	rm -rf traildepot/data && mkdir -p traildepot/data && cat import.sql | sqlite3 traildepot/data/main.db -

.PHONY: init
