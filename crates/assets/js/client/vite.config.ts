import { defineConfig } from "vite";
import dts from "vite-plugin-dts";
import { resolve } from 'path'

// eslint-disable-next-line @typescript-eslint/no-unused-vars
function external(source: string, _importer: string | undefined, _isResolved: boolean): boolean {
  console.log(source)
  return source.startsWith("../bindings");
}

export default defineConfig({
  build: {
    outDir: "./dist",
    minify: false,
    lib: {
      entry: resolve(__dirname, 'src/index.ts'),
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
    }),
  ],
})
