import { defineConfig } from "vite";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  build: {
    lib: {
      entry: fileURLToPath(new URL("./src/index.ts", import.meta.url)),
      name: "BrowserPrint",
      formats: ["es", "iife"],
      fileName: (format) =>
        format === "iife" ? "BrowserPrint.min.js" : "browserprint.js",
    },
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: true,
    minify: "oxc",
    target: "es2020",
    rollupOptions: {
      output: {
        // Default export becomes global `BrowserPrint` in the IIFE build.
        exports: "default",
      },
    },
  },
});
