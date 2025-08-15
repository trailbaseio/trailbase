deploy: build
	cd ../.. && cp target/wasm32-wasip2/release/rust_guest.wasm crates/wasm-runtime/testdata/rust_guest.wasm && cp target/wasm32-wasip2/release/rust_guest.wasm client/testfixture/scripts/

build:
	cargo build --target wasm32-wasip2  --release

.PHONY: build deploy
