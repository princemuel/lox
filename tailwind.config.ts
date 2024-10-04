import twForms from "@tailwindcss/forms";
import twDefaultTheme from "tailwindcss/defaultTheme";
import twPlugin from "tailwindcss/plugin";
import type { Config } from "tailwindcss/types/config";
import twConfig from "./tailwind.json";

export default {
  content: [
    "./app/**/*.{js,ts,jsx,tsx,mdx}",
    "./pages/**/*.{js,ts,jsx,tsx,mdx}",
    "./components/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  future: {
    hoverOnlyWhenSupported: true,
    relativeContentPathsByDefault: true,
  },
  experimental: { optimizeUniversalDefaults: true },
  theme: {
    screens: {
      "3xs": "24em", // @media (min-width: 384px) { ... }
      "2xs": "30em", // @media (min-width: 480px) { ... }
      ...twDefaultTheme.screens,
    },
    extend: {
      borderRadius: { pill: "100vmax" },
      fontFamily: {
        sans: [
          twConfig.theme.fontFamily.sans,
          ...twDefaultTheme.fontFamily.sans,
        ],
      },
      cursor: twConfig.theme.cursor,
      screens: {
        xs: "36em", // @media (min-width: 576px) { ... },
        sm: "40em", // @media (min-width: 640px) { ... }
        md: "48em", // @media (min-width: 768px) { ... }
        lg: "64em", // @media (min-width: 1024px) { ... }
        xl: "80em", // @media (min-width: 1280px) { ... }
        "2xl": "96em", // @media (min-width: 1536px) { ... }
        "3xl": "112.5em", // @media (min-width: 1800px) { ... }
      },
      backgroundImage: {
        "gradient-radial": "radial-gradient(var(--tw-gradient-stops))",
        "gradient-conic":
          "conic-gradient(from 180deg at 50% 50%, var(--tw-gradient-stops))",
      },
    },
  },
  plugins: [
    twForms({ strategy: "base" }),
    twPlugin(({ addVariant }) => {
      addVariant("hocus", ["&:hover", "&:focus"]);
    }),
  ],
} satisfies Config;
