default: format check

static:
	RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-gnu --release --bin trail

format:
	pnpm -r format; \
		cargo +nightly fmt; \
		dart format client/trailbase-dart/ examples/blog/flutter/; \
		txtpbfmt `find . -regex ".*.textproto"`; \
		dotnet format client/trailbase-dotnet; \
		poetry -P client/trailbase-py run black --config client/trailbase-py/pyproject.toml .

check:
	pnpm -r check; \
		cargo clippy --workspace --no-deps; \
		dart analyze client/trailbase-dart examples/blog/flutter; \
		dotnet format client/trailbase-dotnet --verify-no-changes; \
		poetry -P client/trailbase-py run pyright

docker:
	docker build . -t trailbase/trailbase

.PHONY: default format check static
