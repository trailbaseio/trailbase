build_n_data: build data_init

build: dist
	rm -rf dist && pnpm build

data_init:
	rm -rf traildepot/data && mkdir -p traildepot/data && cat import.sql | sqlite3 traildepot/data/main.db -

run:
	cargo run -- run --public-dir dist

.PHONY: init
