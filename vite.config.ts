import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// 在普通浏览器里开发时，界面通过这个地址访问 shibapost-web（npm run server）
const API = process.env.SHIBAPOST_API ?? "http://127.0.0.1:4319";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Tauri 的开发窗口固定连这个端口
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      "/api": { target: API, changeOrigin: false },
      // 「添加到主屏幕」的清单由服务端生成
      "/manifest.webmanifest": { target: API, changeOrigin: false },
    },
  },
  clearScreen: false,
  build: {
    // macOS 的桌面应用用系统的 WebKit：Catalina（10.15）最多只能升到 Safari 15.6。Safari 15 不认的语法
    // （依赖里的类静态块 `static {}` 之类）要在打包时改写，否则整个界面的脚本解析失败、窗口一片空白
    target: ["es2022", "safari15"],
    sourcemap: false,
  },
});
