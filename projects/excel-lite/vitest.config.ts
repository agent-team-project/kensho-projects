import { defineConfig } from "vitest/config";

export default defineConfig({
  root: "ui",
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"]
  }
});
