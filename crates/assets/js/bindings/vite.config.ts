import { resolve } from "path";
import dts from "vite-plugin-dts";
import { defineConfig } from "vite";

export default defineConfig({
  build: {
    outDir: "./dist",
    minify: false,
    lib: {
      entry: resolve(__dirname, "src/index.ts"),
      name: "trailbase-bindings",
      fileName: "index",
      formats: ["es"],
    },
  },
  plugins: [
    dts({
      strictOutput: true,
      bundleTypes: true,
      // Do not include type-declarations in ./tests/.
      include: ["src/**/*.ts"],
    }),
  ],
});
