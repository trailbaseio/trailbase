/**
 * @filename: lint-staged.config.js
 * @type {import('lint-staged').Configuration}
 */
export default {
  '*.rs?(x)': (_files) => [
    'cargo fmt -- --check',
    'pnpm i',
    'cargo clippy --workspace --no-deps',
    'cargo test --workspace -- --show-output',
  ],
  // '*': (files) => [`echo ${files}`, 'exit 1'],
}
