format:
	poetry run black trailbase tests

check:
	poetry run pyright && poetry run flake8

test:
	poetry run pytest -s

publish:
	poetry publish --build

.PHONY: format check test publish
