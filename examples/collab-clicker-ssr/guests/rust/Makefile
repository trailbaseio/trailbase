deploy: clean build
	cp ../../../../target/wasm32-wasip2/release/collab_clicker_ssr_guest.wasm ../../traildepot/scripts/

build:
	cargo build --target wasm32-wasip2  --release

clean:
	rm -rf ../../traildepot/scripts/*.wasm

.PHONY: build deploy clean
