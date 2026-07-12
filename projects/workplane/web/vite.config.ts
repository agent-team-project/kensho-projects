import { defineConfig } from "vitest/config";

export default defineConfig({
  build: {
    sourcemap: true,
    target: "es2023",
  },
  test: {
    environment: "node",
  },
});
