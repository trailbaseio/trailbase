default: types/movie.ts

types/movie.ts: schema/movie.json
	pnpm quicktype -s schema $< -o $@

schema/movie.json:
	cargo run -- --data-dir=../traildepot schema movies --mode insert > $@

clean:
	rm -f types/* schema/*

.PHONY: clean
