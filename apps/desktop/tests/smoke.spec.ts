import { expect, test } from "@playwright/test";

const model = {
  id: "smoke-model",
  name: "smoke-model",
  source: "local",
  format: "gguf",
  local_path: "/tmp/deeplocal-smoke/smoke-model.gguf",
};

test.beforeEach(async ({ page }) => {
  // Keep this smoke test deterministic and offline. The real backend is started
  // by playwright.config.ts, but UI requests use fixtures so no model download
  // or Hugging Face request can happen during the test.
  await page.route("http://127.0.0.1:14567/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const data: Record<string, unknown> = {
      "/health": { status: "ok", name: "deepLocal" },
      "/runtime/hardware": {
        os: "test",
        arch: "x86_64",
        cpu_brand: "Playwright",
        cpu_cores: 4,
        total_ram_bytes: 8e9,
        available_ram_bytes: 4e9,
        gpu: [],
      },
      "/runtime/models": [model],
      "/runtime/models/loaded": [],
      "/runtime/downloads": [],
      "/runtime/chat/conversations": [],
      "/runtime/models/directory": { path: "/tmp/deeplocal-smoke" },
      "/runtime/search-filters": { blocked_keywords: [] },
    };
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(data[path] ?? []) });
  });

  await page.goto("/");
});

test("smoke test navigates through every primary page without downloading models", async ({ page }) => {
  const pages = ["Dashboard", "Models", "Chat", "Server", "Settings"];

  for (const name of pages) {
    await page.getByRole("navigation").getByRole("button", { name, exact: true }).click();
    await expect(page.getByRole("heading", { name, exact: true, level: 1 })).toBeVisible();
  }
});