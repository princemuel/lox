import type { Metadata } from "next";
import { fonts } from "./fonts";
import "./globals.css";

export const metadata: Metadata = {
  title: "L'Ecole School Management Dashboard",
  description: "Next.js School Management System",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className={fonts}>
      <body>{children}</body>
    </html>
  );
}
