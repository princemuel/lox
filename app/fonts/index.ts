import { tw } from "@/helpers/tailwind";
import { Inter } from "next/font/google";

const FontSans = Inter({
  variable: "--font-sans",
  subsets: ["latin"],
});

export const fonts = tw([FontSans.variable]);
