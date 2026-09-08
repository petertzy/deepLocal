import { expect, test, type Page, type Route } from "@playwright/test";

const repo = "meta-llama/example-GGUF";
const filename = "llama-example-Q4_K_M.gguf";
const token = "hf_test_fixture_only";
const authPath = "**/runtime/huggingface/auth-check";
const anonymousPath = "**/runtime/huggingface/anonymous-access";

test.beforeEach(async ({ page }) => {
  await page.route("http://127.0.0.1:14567/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const data: Record<string, unknown> = {
      "/health": { status: "ok" },
      "/runtime/hardware": { os: "darwin", arch: "arm64", cpu_brand: "Apple Silicon", total_ram_bytes: 16e9 },
      "/runtime/models/directory": { path: "/local/models" },
      "/runtime/search-filters": { blocked_keywords: ["example-filter"] },
      "/runtime/huggingface/search": [{
        repo,
        downloads: 1000,
        likes: 50,
        files: [
          { filename, size_bytes: 3e9 },
          ...Array.from({ length: 12 }, (_, index) => ({ filename: `model-${index}-Q8_0.gguf`, size_bytes: 4e9 + index })),
          { filename: "mmproj-F16.gguf", size_bytes: 1e8 },
        ],
      }],
    };
    await route.fulfill({ json: data[path] ?? [] });
  });
  await page.goto("/");
});

async function navigate(page: Page, name: string) {
  // Avoid scrolling to the navigation button before testing scroll restoration.
  await page.getByRole("navigation").getByRole("button", { name, exact: true }).dispatchEvent("click");
  await expect(page.getByRole("heading", { name, exact: true, level: 1 })).toBeVisible();
}

async function search(page: Page) {
  await navigate(page, "Models");
  await page.getByRole("textbox", { name: "Search Hugging Face models" }).fill("Llama example");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.locator(".searchModelCard")).toHaveCount(13);
}

function modelRow(page: Page) {
  return page.locator(".searchModelCard").filter({ has: page.getByRole("heading", { name: filename, exact: true }) });
}

function authResult(access: boolean) {
  return {
    token_valid: true,
    repository_checked: true,
    repository_access: access,
    user: { name: "test-user", display_name: "Test User" },
    message: access ? "Token can access this file." : `Access to ${repo} requires accepting its license and read access.`,
  };
}

test("preserves search results, drafts, diagnostics, and scroll positions between pages", async ({ page }) => {
  let searches = 0;
  page.on("request", (request) => { if (request.url().includes("huggingface/search?")) searches++; });
  await page.route(anonymousPath, (route) => route.fulfill({ json: { status: "public", message: "Anonymous download available." } }));
  await page.route(authPath, (route) => route.fulfill({ json: { token_valid: true, user: { name: "test-user" }, message: "Token is valid." } }));
  await search(page);
  await modelRow(page).getByRole("button", { name: /^Check access/ }).click();
  await expect(modelRow(page).getByRole("status")).toContainText("Public");
  await page.getByRole("combobox", { name: "Sort" }).selectOption("name");
  await page.getByRole("checkbox", { name: "Show auxiliary files" }).check();
  await page.getByRole("textbox", { name: "Model ID", exact: true }).fill("draft-id");
  await page.getByRole("textbox", { name: "GGUF file path", exact: true }).fill("/local/models/draft.gguf");
  await page.locator(".searchResults").evaluate((element) => {
    element.scrollTop = 180;
    element.dispatchEvent(new Event("scroll", { bubbles: true }));
  });
  await page.evaluate(() => window.scrollTo(0, 300));
  const searchScroll = await page.locator(".searchResults").evaluate((element) => element.scrollTop);
  const pageScroll = await page.evaluate(() => window.scrollY);
  await navigate(page, "Settings");
  await expect(page.locator(".workspace > header")).not.toContainText("GGUF repositories");
  await expect(page.getByRole("textbox", { name: "Repository", exact: true })).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "GGUF filename", exact: true })).toHaveCount(0);
  await page.getByRole("textbox", { name: "Custom blocked keyword" }).fill("unfinished-keyword");
  await page.getByRole("button", { name: "Check Token", exact: true }).click();
  await expect(page.getByRole("region", { name: "Hugging Face account" })).toContainText("test-user");
  await navigate(page, "Models");
  await expect(page.locator(".workspace > header")).toContainText("Found 1 GGUF repositories.");
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(pageScroll);
  await expect.poll(() => page.locator(".searchResults").evaluate((element) => element.scrollTop)).toBe(searchScroll);
  await expect(page.getByRole("textbox", { name: "Search Hugging Face models" })).toHaveValue("Llama example");
  await expect(page.getByRole("combobox", { name: "Sort" })).toHaveValue("name");
  await expect(page.getByRole("checkbox", { name: "Show auxiliary files" })).toBeChecked();
  await expect(page.getByRole("textbox", { name: "Model ID", exact: true })).toHaveValue("draft-id");
  await expect(page.getByRole("textbox", { name: "GGUF file path", exact: true })).toHaveValue("/local/models/draft.gguf");
  await expect(modelRow(page).getByRole("status")).toContainText("Public");
  await expect(page.locator(".searchModelCard")).toHaveCount(14);
  expect(searches).toBe(1);
  await navigate(page, "Settings");
  await expect(page.getByRole("textbox", { name: "Custom blocked keyword" })).toHaveValue("unfinished-keyword");
  await expect(page.getByRole("region", { name: "Hugging Face account" })).toContainText("test-user");

  await page.reload();
  await expect(page.getByRole("heading", { name: "Settings", exact: true, level: 1 })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Custom blocked keyword" })).toHaveValue("unfinished-keyword");
  await expect(page.getByRole("region", { name: "Hugging Face account" })).toContainText("test-user");
  await navigate(page, "Models");
  await expect(page.getByRole("textbox", { name: "Search Hugging Face models" })).toHaveValue("Llama example");
  await expect(page.getByRole("combobox", { name: "Sort" })).toHaveValue("name");
  await expect(page.getByRole("checkbox", { name: "Show auxiliary files" })).toBeChecked();
  await expect(page.getByRole("textbox", { name: "Model ID", exact: true })).toHaveValue("draft-id");
  await expect(page.getByRole("textbox", { name: "GGUF file path", exact: true })).toHaveValue("/local/models/draft.gguf");
  await expect(modelRow(page).getByRole("status")).toContainText("Public");
  await expect(modelRow(page).getByRole("link", { name: repo, exact: true }).first()).toHaveAttribute("href", `https://huggingface.co/${repo}`);
  await expect.poll(() => page.locator(".searchResults").evaluate((element) => element.scrollTop)).toBe(searchScroll);
  expect(searches).toBe(1);
});

test("checks the chosen search file using the current token and links to its repository", async ({ page }) => {
  await navigate(page, "Settings");
  await page.getByLabel("Hugging Face token", { exact: true }).fill(token);
  let requestBody: unknown;
  await page.route(authPath, (route) => {
    requestBody = route.request().postDataJSON();
    return route.fulfill({ json: authResult(false) });
  });
  await search(page);
  const row = modelRow(page);
  await expect(row.getByRole("link", { name: repo, exact: true })).toHaveCount(1);
  await expect(row.getByRole("link", { name: repo, exact: true }).first()).toHaveAttribute("href", `https://huggingface.co/${repo}`);
  await row.getByRole("button", { name: /^Check access/ }).click();
  await expect(row.getByRole("status")).toContainText("Token valid, no access");
  expect(requestBody).toEqual({ token, repo, filename, use_env_token: false });
  await expect(row.getByRole("link", { name: repo, exact: true })).toHaveCount(2);
  await expect(page.locator("body")).not.toContainText(token);
  await row.getByRole("button", { name: "Token settings" }).click();
  await expect(page.getByLabel("Hugging Face token", { exact: true })).toHaveValue(token);
  await page.getByLabel("Hugging Face token", { exact: true }).fill("");
  let anonymousBody: unknown;
  await page.route(anonymousPath, (route) => {
    anonymousBody = route.request().postDataJSON();
    return route.fulfill({ json: { status: "authentication_required", message: "Authentication required. Check repository access." } });
  });
  await navigate(page, "Models");
  await expect(row.getByRole("status")).toHaveCount(0);
  await row.getByRole("button", { name: /^Check access/ }).click();
  await expect(row.getByRole("status")).toContainText("Sign-in required");
  expect(anonymousBody).toEqual({ repo, filename });
});

test("restores the active chat page and unsent chat draft after refresh", async ({ page }) => {
  await navigate(page, "Chat");
  await page.getByRole("textbox", { name: "Chat prompt" }).fill("keep this local draft");
  await page.getByRole("checkbox", { name: "Streaming" }).uncheck();
  await page.getByRole("button", { name: "Show conversations" }).click();
  await expect(page.locator(".conversationList")).toBeVisible();
  await page.reload();
  await expect(page.getByRole("heading", { name: "Chat", exact: true, level: 1 })).toBeVisible();
  await expect(page.locator(".conversationList")).toBeVisible();
  await page.getByRole("button", { name: "Back to current chat" }).click();
  await expect(page.getByRole("textbox", { name: "Chat prompt" })).toHaveValue("keep this local draft");
  await expect(page.getByRole("checkbox", { name: "Streaming" })).not.toBeChecked();
});

test("finishes an in-progress check while away, and ignores results for an old token", async ({ page }) => {
  let pendingRoute: Route;
  await page.route(anonymousPath, (route) => { pendingRoute = route; });
  await search(page);
  const row = modelRow(page);
  await row.getByRole("button", { name: /^Check access/ }).click();
  await expect.poll(() => !!pendingRoute).toBe(true);
  await navigate(page, "Settings");
  await pendingRoute!.fulfill({ json: { status: "public", message: "Anonymous download available." } });
  await navigate(page, "Models");
  await expect(row.getByRole("status")).toContainText("Public");

  let staleRoute: Route;
  await page.route(authPath, (route) => { staleRoute = route; });
  await navigate(page, "Settings");
  await page.getByLabel("Hugging Face token", { exact: true }).fill(token);
  await navigate(page, "Models");
  await row.getByRole("button", { name: /^Check access/ }).click();
  await expect.poll(() => !!staleRoute).toBe(true);
  await navigate(page, "Settings");
  await page.getByLabel("Hugging Face token", { exact: true }).fill("hf_replacement_fixture");
  await staleRoute!.fulfill({ json: authResult(true) });
  await navigate(page, "Models");
  await expect(row.getByRole("status")).toHaveCount(0);
  await expect(row.getByRole("button", { name: /^Check access/ })).toBeEnabled();
});

test("keeps token checks pending across navigation and recovers from a network error", async ({ page }) => {
  let pendingRoute: Route;
  await page.route(authPath, (route) => { pendingRoute = route; });
  await navigate(page, "Settings");
  await page.getByLabel("Hugging Face token", { exact: true }).fill(token);
  await page.getByRole("button", { name: "Check Token", exact: true }).click();
  await expect.poll(() => !!pendingRoute).toBe(true);
  await navigate(page, "Models");
  await pendingRoute!.abort("failed");
  await navigate(page, "Settings");
  await expect(page.getByRole("region", { name: "Hugging Face account" })).toContainText("Check your connection");
  await expect(page.getByRole("button", { name: "Check Token", exact: true })).toBeEnabled();
  await page.route(authPath, (route) => route.fulfill({ json: { token_valid: true, user: { name: "test-user" }, message: "Token is valid." } }));
  await page.getByRole("button", { name: "Check Token", exact: true }).click();
  await expect(page.getByRole("region", { name: "Hugging Face account" })).toContainText("test-user");
});

for (const width of [1440, 768, 390]) {
  test(`access controls and settings fit at ${width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 1000 });
    await page.route(anonymousPath, (route) => route.fulfill({ json: {
      status: "restricted",
      message: "Access restricted. Check the repository license and access requirements on Hugging Face.",
    } }));
    await search(page);
    const row = modelRow(page);
    await row.getByRole("button", { name: /^Check access/ }).click();
    await expect(row.getByRole("status")).toContainText("Access restricted");
    const check = await row.getByRole("button", { name: /^Check access/ }).boundingBox();
    const download = await row.getByRole("button", { name: "Download", exact: true }).boundingBox();
    expect(check!.x + check!.width).toBeLessThanOrEqual(download!.x);
    expect(download!.x + download!.width).toBeLessThanOrEqual(width);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    await row.screenshot({ path: testInfo.outputPath(`model-access-${width}.png`) });

    await page.route("**/runtime/downloads", (route) => route.fulfill({ json: [{
      id: "download-fixture", repo, filename, status: "cancelling", downloaded_bytes: 1e8, total_bytes: 3e9,
    }] }));
    await expect(row.getByRole("button", { name: "Stopping", exact: true })).toBeVisible();
    expect(await row.locator("button").evaluateAll((buttons) => buttons.every((button) => button.scrollWidth <= button.clientWidth))).toBe(true);
    await row.screenshot({ path: testInfo.outputPath(`active-download-${width}.png`) });
    await navigate(page, "Settings");
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    await expect(page.getByRole("button", { name: "Check Token", exact: true })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath(`settings-${width}.png`), fullPage: true });
  });
}
