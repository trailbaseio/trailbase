import { defineConfig } from "vite";

export default defineConfig({
  build: {
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
