import { defineConfig } from "vite";
import { resolve } from "path";

const backend = process.env.TAURI_ENV_PLATFORM === "windows" ? 1421 : 1420;

export default defineConfig({
  clearScreen: false,
  server: {
    port: backend,
    strictPort: true,
    host: "127.0.0.1",
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        pet: resolve(__dirname, "pet.html"),
      },
    },
  },
});
