deploy: clean build
	cp ../../../../target/wasm32-wasip2/release/wasm_rust_guest_testfixture.wasm ../../scripts/

build:
	cargo build --target wasm32-wasip2  --release

clean:
	rm -rf ../../scripts/*.wasm

.PHONY: build deploy clean
