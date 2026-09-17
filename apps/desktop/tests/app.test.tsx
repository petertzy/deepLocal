import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => "http://127.0.0.1:14567" }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => undefined }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: async () => undefined }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: async () => null }));

import { App } from "../src/main";

const model = {
  id: "fixture-model",
  name: "fixture-model",
  source: "local",
  format: "gguf",
  local_path: "/models/fixture-model.gguf",
};

const searchResult = {
  repo: "meta-llama/example-GGUF",
  downloads: 1000,
  likes: 50,
  files: [
    { filename: "llama-example-Q4_K_M.gguf", size_bytes: 3_000_000_000 },
    { filename: "mmproj-F16.gguf", size_bytes: 100_000_000 },
  ],
};

function jsonResponse(value: unknown, ok = true) {
  return Promise.resolve({ ok, json: async () => value, text: async () => JSON.stringify(value) } as Response);
}

function apiState(options: { models?: unknown[]; loaded?: unknown[]; downloads?: unknown[]; conversations?: unknown[] } = {}) {
  return {
    "/health": { status: "ok" },
    "/runtime/hardware": {
      os: "test",
      arch: "test",
      cpu_brand: "test",
      total_ram_bytes: 16_000_000_000,
      available_ram_bytes: 16_000_000_000,
    },
    "/runtime/models": options.models ?? [],
    "/runtime/models/loaded": options.loaded ?? [],
    "/runtime/downloads": options.downloads ?? [],
    "/runtime/chat/conversations": options.conversations ?? [],
    "/runtime/models/directory": { path: "/models" },
  };
}

function installFetch(state: Record<string, unknown>, search = false) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    const path = new URL(url, "http://127.0.0.1:14567").pathname;
    if (path === "/runtime/huggingface/download") {
      state["/runtime/downloads"] = [{
        id: "download-1",
        repo: searchResult.repo,
        filename: searchResult.files[0].filename,
        status: "downloading",
        downloaded_bytes: 1_000_000,
        total_bytes: 3_000_000_000,
      }];
      return jsonResponse({ status: "queued" });
    }
    if (path === "/runtime/huggingface/search" && search) return jsonResponse([searchResult]);
    return jsonResponse(state[path] ?? []);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

async function navigate(name: string) {
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
  await waitFor(() => expect(screen.getByRole("heading", { name, level: 1 })).toBeInTheDocument());
}

beforeEach(() => {
  window.localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("core frontend flows", () => {
  it("renders model search results", async () => {
    installFetch(apiState(), true);
    render(<App />);
    await navigate("Models");

    await userEvent.type(screen.getByRole("textbox", { name: "Search Hugging Face models" }), "Llama");
    await userEvent.click(screen.getByRole("button", { name: /^Search$/ }));

    expect(await screen.findByRole("heading", { name: "llama-example-Q4_K_M.gguf" })).toBeInTheDocument();
    expect(screen.getByText("meta-llama/example-GGUF")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "mmproj-F16.gguf" })).not.toBeInTheDocument();
  });

  it("shows download state transitions from queued to downloading", async () => {
    const state = apiState() as Record<string, unknown>;
    const fetchMock = installFetch(state, true);
    render(<App />);
    await navigate("Models");
    await userEvent.type(screen.getByRole("textbox", { name: "Search Hugging Face models" }), "Llama");
    await userEvent.click(screen.getByRole("button", { name: /^Search$/ }));

    const card = (await screen.findByRole("heading", { name: "llama-example-Q4_K_M.gguf" })).closest("article")!;
    await userEvent.click(within(card).getByRole("button", { name: /^Download$/ }));
    expect(await within(card).findByText(/Preparing|Starting|Downloading/)).toBeInTheDocument();

    await waitFor(() => expect(within(card).getByText("Downloading")).toBeInTheDocument());
    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining("/runtime/huggingface/download"), expect.objectContaining({ method: "POST" }));
  });

  it("warns before downloading a search result larger than available RAM", async () => {
    const riskySearchResult = {
      ...searchResult,
      files: [{ filename: "gemma-large-Q4_K_M.gguf", size_bytes: 20_000_000_000 }],
    };
    const state = apiState() as Record<string, unknown>;
    const fetchMock = installFetch(state, true);
    fetchMock.mockImplementation(async (input: RequestInfo | URL) => {
      const path = new URL(String(input), "http://127.0.0.1:14567").pathname;
      if (path === "/runtime/huggingface/search") return jsonResponse([riskySearchResult]);
      if (path === "/runtime/huggingface/download") return jsonResponse({ status: "queued" });
      return jsonResponse(state[path] ?? []);
    });
    render(<App />);
    await navigate("Models");
    await userEvent.type(screen.getByRole("textbox", { name: "Search Hugging Face models" }), "gemma");
    await userEvent.click(screen.getByRole("button", { name: /^Search$/ }));

    const card = await screen.findByRole("heading", { name: "gemma-large-Q4_K_M.gguf" });
    expect(within(card.closest("article")!).getByText(/May not fit/)).toBeInTheDocument();
    await userEvent.click(within(card.closest("article")!).getByRole("button", { name: /^Download$/ }));
    expect(await screen.findByRole("heading", { name: "Model may not fit" })).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalledWith(expect.stringContaining("/runtime/huggingface/download"), expect.anything());

    await userEvent.click(screen.getByRole("button", { name: "Download anyway" }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/runtime/huggingface/download"),
      expect.objectContaining({ method: "POST" }),
    ));
  });

  it("does not mark every search result risky when available RAM is unknown", async () => {
    const state = apiState() as Record<string, unknown>;
    state["/runtime/hardware"] = {
      ...(state["/runtime/hardware"] as Record<string, unknown>),
      available_ram_bytes: 0,
    };
    const fetchMock = installFetch(state, true);
    render(<App />);
    await navigate("Models");
    await userEvent.type(screen.getByRole("textbox", { name: "Search Hugging Face models" }), "gemma");
    await userEvent.click(screen.getByRole("button", { name: /^Search$/ }));

    const card = (await screen.findByRole("heading", { name: "llama-example-Q4_K_M.gguf" })).closest("article")!;
    expect(within(card).queryByText(/May not fit/)).not.toBeInTheDocument();
    await userEvent.click(within(card).getByRole("button", { name: /^Download$/ }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/runtime/huggingface/download"),
      expect.objectContaining({ method: "POST" }),
    ));
    expect(screen.queryByRole("heading", { name: "Model may not fit" })).not.toBeInTheDocument();
  });

  it("shows the chat empty state when no model is loaded", async () => {
    installFetch(apiState());
    render(<App />);
    await navigate("Chat");

    expect(await screen.findByText("Load a model to start chatting")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open models" })).toBeInTheDocument();
  });

  it("warns before loading a model larger than available RAM and allows override", async () => {
    const riskyModel = { ...model, size_bytes: 20_000_000_000 };
    const state = apiState({ models: [riskyModel] });
    const fetchMock = installFetch(state);
    render(<App />);
    await navigate("Models");

    const card = screen.getByRole("heading", { name: riskyModel.name }).closest("article")!;
    await userEvent.click(within(card).getByRole("button", { name: /^Load$/ }));
    expect(await screen.findByRole("heading", { name: "Large model may not fit" })).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalledWith(expect.stringContaining("/runtime/models/load"), expect.anything());

    await userEvent.click(screen.getByRole("button", { name: "Load anyway" }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/runtime/models/load"),
      expect.objectContaining({ method: "POST" }),
    ));
  });

  it("loads a model immediately when it fits in available RAM", async () => {
    const fittingModel = { ...model, size_bytes: 1_000_000_000 };
    const state = apiState({ models: [fittingModel] });
    const fetchMock = installFetch(state);
    render(<App />);
    await navigate("Models");

    const card = screen.getByRole("heading", { name: fittingModel.name }).closest("article")!;
    await userEvent.click(within(card).getByRole("button", { name: /^Load$/ }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/runtime/models/load"),
      expect.objectContaining({ method: "POST" }),
    ));
    expect(screen.queryByRole("heading", { name: "Large model may not fit" })).not.toBeInTheDocument();
  });

  it("shows the loaded-model chat state", async () => {
    installFetch(apiState({ models: [model], loaded: [{ id: model.id, backend: "mock", status: "loaded" }] }));
    render(<App />);
    await navigate("Chat");

    await waitFor(() => expect(screen.getByRole("combobox")).toHaveValue(model.id));
    expect(screen.queryByText("Load a model to start chatting")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
  });
});