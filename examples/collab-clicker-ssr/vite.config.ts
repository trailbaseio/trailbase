import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";

// https://vite.dev/config/
export default defineConfig({
  plugins: [solid({ ssr: true }), tailwindcss()],
  ssr: {
    noExternal: true,
  },
});
