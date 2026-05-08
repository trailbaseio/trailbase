import { defineConfig } from "vite";

export default defineConfig({
  build: {
    outDir: "./dist",
    minify: false,
    lib: {
      entry: "./src/index.ts",
      name: "runtime",
      fileName: "index",
      formats: ["es"],
    },
    rollupOptions: {
      preserveEntrySignatures: 'strict',
      external: /(wasi|trailbase):.*/,
    },
  },
})
