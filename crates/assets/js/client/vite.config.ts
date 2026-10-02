import { defineConfig } from "vite";
import dts from "vite-plugin-dts";
import { resolve } from "path";

export default defineConfig({
  build: {
    outDir: "./dist",
    minify: false,
    lib: {
      entry: resolve(__dirname, "src/index.ts"),
      name: "trailbase",
      fileName: "index",
      formats: ["es"],
    },
  },
  plugins: [
    dts({
      strictOutput: true,
      // copyDtsFiles: true,
      // staticImport: true,
      // insertTypesEntry: true,
      bundleTypes: true,
      // Do not include type-declarations in ./tests/.
      include: ["src/*"],
    }),
  ],
});
