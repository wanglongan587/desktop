import { defineConfig } from 'vite'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import * as path from 'node:path'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: [
      { find: "@", replacement: path.resolve(__dirname, "./web") },
      { find: /^@ora\/app-shell$/, replacement: path.resolve(__dirname, "../../packages/app-shell/src/index.ts") },
      { find: /^@ora\/chat$/, replacement: path.resolve(__dirname, "../../packages/chat/src/index.ts") },
      { find: /^@ora\/contracts$/, replacement: path.resolve(__dirname, "../../packages/contracts/src/index.ts") },
      { find: /^@ora\/ui$/, replacement: path.resolve(__dirname, "../../packages/ui/src/index.ts") },
    ],
  },
  server: {
    host: "0.0.0.0",
    // Exclude Rust build output from the Vite file watcher. On Windows, cargo
    // locks build-script binaries under target/debug/build while compiling, so
    // watching those paths makes Vite crash with EBUSY during `tauri dev`.
    watch: {
      ignored: ["**/target/**", "**/.data/**", "**/.cache/**"],
    },
    proxy: {
      "/api": {
        target: "http://localhost:21688",
        changeOrigin: true,
      },
    },
  },
})
