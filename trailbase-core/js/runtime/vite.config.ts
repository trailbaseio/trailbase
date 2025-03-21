import { defineConfig } from "vite";

import dts from 'vite-plugin-dts'

export default defineConfig({
  plugins: [
    dts({ rollupTypes: true }),
  ],
  build: {
    outDir: "./dist",
    minify: false,
    lib: {
      entry: "./src/index.ts",
      name: "runtime",
      fileName: "index",
      formats: ["es"],
    },
  },
})
