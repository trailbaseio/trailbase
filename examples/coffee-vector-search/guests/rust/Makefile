deploy: clean build
	cp ../../../../target/wasm32-wasip2/release/coffee_vector_search_guest.wasm ../../traildepot/wasm/

build:
	cargo build --target wasm32-wasip2  --release

clean:
	rm -rf ../../traildepot/scripts/*.wasm

.PHONY: build deploy clean
