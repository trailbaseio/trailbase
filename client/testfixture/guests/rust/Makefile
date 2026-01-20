TARGET=wasm32-wasip2
#TARGET=wasm32-unknown-unknown

deploy: clean build
	cp ../../../../target/${TARGET}/release/wasm_rust_guest_testfixture.wasm ../../wasm/wasm_guest_testfixture.wasm

build:
	cargo build --target ${TARGET} --release

clean:
	rm -rf ../../wasm/wasm_rust_guest_testfixture.wasm

.PHONY: build deploy clean
