import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// 在普通浏览器里开发时，界面通过这个地址访问 postflow-web（npm run server）
const API = process.env.POSTFLOW_API ?? "http://127.0.0.1:4319";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Tauri 的开发窗口固定连这个端口
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      "/api": { target: API, changeOrigin: false },
    },
  },
  clearScreen: false,
  build: {
    target: "es2022",
    sourcemap: false,
  },
});
