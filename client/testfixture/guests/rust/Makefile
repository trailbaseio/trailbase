deploy: clean build
	cp ../../../../target/wasm32-wasip2/release/wasm_rust_guest_testfixture.wasm ../../wasm/

build:
	cargo build --target wasm32-wasip2  --release

clean:
	rm -rf ../../wasm/wasm_rust_guest_testfixture.wasm

.PHONY: build deploy clean
