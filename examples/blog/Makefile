outputs = \
	  web/types/article.ts \
	  web/types/profile.ts \
	  web/types/new_profile.ts \
	  flutter/lib/types/article.dart \
	  flutter/lib/types/profile.dart \
	  flutter/lib/types/new_profile.dart

types: $(outputs)

schema/article.json:
	cargo run -- schema articles_view --mode select > $@
web/types/article.ts: schema/article.json
	pnpm quicktype -s schema $< -o $@
flutter/lib/types/article.dart: schema/article.json
	pnpm quicktype -s schema $< -o $@

schema/new_article.json:
	cargo run -- schema articles_view --mode insert > $@
web/types/new_article.ts: schema/new_article.json
	pnpm quicktype -s schema $< -o $@

schema/profile.json:
	cargo run -- schema profiles_view --mode select > $@
web/types/profile.ts: schema/profile.json
	pnpm quicktype -s schema $< -o $@
flutter/lib/types/profile.dart: schema/profile.json
	pnpm quicktype -s schema $< -o $@

schema/new_profile.json:
	cargo run -- schema profiles --mode insert > $@
web/types/new_profile.ts: schema/new_profile.json
	 pnpm quicktype -s schema $< -o $@
flutter/lib/types/new_profile.dart: schema/new_profile.json
	pnpm quicktype -s schema $< -o $@

clean_data:
	rm -f traildepot/data/*

clean_types:
	rm -f schema/* web/types/* flutter/lib/types/*

clean: clean_data clean_types

.PHONY: clean clean_data clean_types
