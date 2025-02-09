import type { Config } from "tailwindcss";

import starlightPlugin from "@astrojs/starlight-tailwind";

// Generated color palettes
const accent = {
  200: "#92d1fe",
  600: "#0073aa",
  900: "#003653",
  950: "#00273d",
};
const gray = {
  100: "#f3f7f9",
  200: "#e7eff2",
  300: "#bac4c8",
  400: "#7b8f96",
  500: "#495c62",
  700: "#2a3b41",
  800: "#182a2f",
  900: "#121a1c",
};

export default {
  content: ["./src/**/*.{astro,html,js,jsx,md,mdx,svelte,ts,tsx,vue}"],
  theme: {
    extend: {
      colors: {
        accent,
        gray,
      },
    },
  },
  plugins: [starlightPlugin()],
} satisfies Config;
