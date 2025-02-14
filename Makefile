default: format check

static:
	RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-gnu --release --bin trail

format:
	pnpm -r format; \
		cargo +nightly fmt; \
		dart format client/trailbase-dart/ examples/blog/flutter/; \
		# Don't mess with TrailBase writing config.textproto
		txtpbfmt `find . -regex ".*.textproto" | grep -v config.textproto`; \
		dotnet format client/trailbase-dotnet/src; \
	       	dotnet format client/trailbase-dotnet/test; \
		poetry -C client/trailbase-py run black --config pyproject.toml .

check:
	pnpm -r check; \
		cargo clippy --workspace --no-deps; \
		dart analyze client/trailbase-dart examples/blog/flutter; \
		dotnet format client/trailbase-dotnet/src --verify-no-changes; \
		dotnet format client/trailbase-dotnet/test --verify-no-changes; \
		poetry -C client/trailbase-py run pyright

docker:
	docker buildx build --platform linux/arm64,linux/amd64 --output=type=registry -t trailbase/trailbase:latest .

cloc:
	cloc --not-match-d=".*(/target|/dist|/node_modules|/vendor|.astro|.venv|/traildepot|/flutter|/assets|lock|_benchmark|/bin|/obj).*" .

.PHONY: default format check static docker cloc
