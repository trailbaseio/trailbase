import animate from "tailwindcss-animate";
import typography from "@tailwindcss/typography";
import type { Config } from "tailwindcss";

import { commonTailwindConfig } from "../styles/tailwind.config.mjs";

export default {
  content: ["./src/**/*.{astro,html,js,jsx,md,mdx,ts,tsx}"],
  presets: [commonTailwindConfig],
  plugins: [animate, typography],
} satisfies Config;
