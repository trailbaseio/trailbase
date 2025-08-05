default: format check

static:
	RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-gnu --release --bin trail

format:
	pnpm -r format; \
		cargo +nightly fmt; \
		dart format client/dart docs/examples/record_api_dart examples/blog/flutter; \
		# Don't mess with TrailBase writing config.textproto
		txtpbfmt `find . -regex ".*.textproto" | grep -v config.textproto`; \
		dotnet format client/dotnet/trailbase; \
	       	dotnet format client/dotnet/test; \
		poetry -C client/python run black --config pyproject.toml .; \
		swift format -r -i client/swift/trailbase/**/*.swift; \
		gofmt -w **/*.go;

check:
	pnpm -r check; \
		cargo clippy --workspace --no-deps; \
		dart analyze; \
		dotnet format client/dotnet/trailbase --verify-no-changes; \
		dotnet format client/dotnet/test --verify-no-changes; \
		poetry -C client/python run pyright

docker:
	docker buildx build --platform linux/arm64,linux/amd64 --output=type=registry -t trailbase/trailbase:latest .

openapi:
	cargo run -- openapi print > docs/openapi/schema.json

cloc:
	cloc --not-match-d=".*(/target|/dist|/node_modules|/vendor|.astro|.build|.venv|/traildepot|/flutter|/assets|lock|_benchmark|/bin|/obj).*" .

publish_crates:
	cargo +nightly -Z package-workspace publish --no-verify \
		-p trailbase-build \
		-p trailbase-assets \
		-p trailbase-qs \
		-p trailbase-sqlean \
		-p trailbase-refinery \
		-p trailbase-extension \
		-p trailbase-schema \
		-p trailbase-sqlite \
		-p trailbase-js \
		-p trailbase

.PHONY: default format check static docker openapi cloc publish_crates
