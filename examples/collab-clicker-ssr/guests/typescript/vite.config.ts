import { defineConfig } from "vite";

export default defineConfig({
  build: {
    outDir: "./dist",
    minify: false,
    lib: {
      entry: "./src/component.js",
      name: "runtime",
      fileName: "index",
      formats: ["es"],
    },
    rollupOptions: {
      external: (source) => {
        return source.startsWith("wasi:") || source.startsWith("trailbase:");
      },
    },
  },
})
