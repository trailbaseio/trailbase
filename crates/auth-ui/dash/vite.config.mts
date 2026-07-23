import { defineConfig } from "vite";

import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";
import tsconfigPaths from "vite-tsconfig-paths";

export default defineConfig({
  base: "/_/auth/admin/ui",
  plugins: [solid(), tsconfigPaths(), tailwindcss()],
});
