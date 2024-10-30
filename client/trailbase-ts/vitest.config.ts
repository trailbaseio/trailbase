import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    globals: true,
    environment: "jsdom",
    // We do not include transitively, since we rely on our own runner for
    // executing tests/integration/** instead.
    include: [
      'tests/*.test.ts',
      'tests/*.bench.ts',
    ],
  },
})
