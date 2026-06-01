import { defineConfig } from "vite";

// Tauri 期望固定端口；clearScreen=false 保留 Rust 日志。
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2021",
    outDir: "dist",
  },
});
