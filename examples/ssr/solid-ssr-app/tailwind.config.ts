import type { Config } from "tailwindcss";

import { fontFamily } from "tailwindcss/defaultTheme";

// Colors from starlight.
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
  content: ["index.html", "./src/**/*.{html,js,jsx,md,mdx,ts,tsx}"],
  theme: {
    extend: {
      colors: { accent, gray },
      fontFamily: {
        sans: ["Inter", ...fontFamily.sans],
      },
    },
  },
  plugins: [],
} satisfies Config;
