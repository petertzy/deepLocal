import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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

function apiState(options: { models?: unknown[]; loaded?: unknown[]; downloads?: unknown[]; conversations?: unknown[]; documents?: unknown[] } = {}) {
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
    "/runtime/chat/presets": [],
    "/runtime/documents": options.documents ?? [],
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

  it("indexes a selected local text document", async () => {
    const state = apiState() as Record<string, unknown>;
    const fetchMock = installFetch(state);
    let uploadedDocument: Record<string, unknown> | undefined;
    fetchMock.mockImplementation(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input), "http://127.0.0.1:14567").pathname;
      if (path === "/runtime/documents" && init?.method === "POST") {
        const documentRequest = JSON.parse(String(init.body)) as Record<string, unknown>;
        uploadedDocument = documentRequest;
        state["/runtime/documents"] = [{
          id: "document-1",
          name: documentRequest.name,
          source_key: documentRequest.source_key,
          character_count: String(documentRequest.content).length,
          chunk_count: 1,
          created_at: "2026-09-18T09:00:00.000Z",
          updated_at: "2026-09-18T09:00:00.000Z",
        }];
        return jsonResponse(state["/runtime/documents"]);
      }
      return jsonResponse(state[path] ?? []);
    });
    render(<App />);
    await navigate("Documents");

    const picker = document.querySelector<HTMLInputElement>('input[type="file"]');
    expect(picker).not.toBeNull();
    await userEvent.upload(picker!, new File(["The release stays entirely local."], "release-notes.md", { type: "text/markdown" }));

    expect(await screen.findByText("release-notes.md")).toBeInTheDocument();
    expect(uploadedDocument).toMatchObject({
      name: "release-notes.md",
      content: "The release stays entirely local.",
      source_key: "browser:release-notes.md",
    });
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

    await waitFor(() => expect(screen.getByRole("combobox", { name: "Chat model" })).toHaveValue(model.id));
    expect(screen.queryByText("Load a model to start chatting")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
  });

  it("creates and selects a prompt preset and inserts its template", async () => {
    const state = apiState({ models: [model], loaded: [{ id: model.id, backend: "mock", status: "loaded" }] }) as Record<string, unknown>;
    const presets: Record<string, unknown>[] = [];
    const fetchMock = installFetch(state);
    fetchMock.mockImplementation(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input), "http://127.0.0.1:14567").pathname;
      if (path === "/runtime/chat/presets" && init?.method === "POST") {
        const body = JSON.parse(String(init.body)) as Record<string, unknown>;
        const preset = { ...body, id: "preset-review", created_at: "2026-09-18T09:00:00Z", updated_at: "2026-09-18T09:00:00Z" };
        presets.splice(0, presets.length, preset);
        return jsonResponse(preset);
      }
      if (path === "/runtime/chat/presets") return jsonResponse(presets);
      return jsonResponse(state[path] ?? []);
    });
    render(<App />);
    await navigate("Chat");
    await userEvent.click(screen.getByRole("button", { name: "Manage prompt presets" }));
    await userEvent.type(screen.getByPlaceholderText("e.g. Code reviewer"), "Code reviewer");
    await userEvent.type(screen.getByPlaceholderText("Instructions that guide the assistant's responses"), "Review carefully.");
    fireEvent.change(screen.getByPlaceholderText("Reusable text to insert into the chat input"), { target: { value: "Review this code: {{code}}" } });
    await userEvent.click(screen.getByRole("button", { name: "Save preset" }));

    expect(await screen.findByRole("option", { name: "Code reviewer" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Prompt preset" })).toHaveValue("preset-review");
    fireEvent.change(screen.getByRole("textbox", { name: "Chat prompt" }), { target: { value: "" } });
    await userEvent.click(screen.getByRole("button", { name: "Insert template" }));
    expect(screen.getByRole("textbox", { name: "Chat prompt" })).toHaveValue("Review this code: {{code}}");
    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining("/runtime/chat/presets"), expect.objectContaining({ method: "POST" }));
  });

  it("includes the selected system prompt in chat completion requests", async () => {
    window.localStorage.setItem("deeplocal:chat-streaming", "false");
    const conversation = {
      id: "chat-preset-test",
      title: "Prompt test",
      model_id: model.id,
      messages: [],
      created_at: "2026-09-18T09:00:00Z",
      updated_at: "2026-09-18T09:00:00Z",
    };
    window.localStorage.setItem("deeplocal:active-chat-conversation", conversation.id);
    const preset = {
      id: "preset-instructions",
      name: "Patient tutor",
      system_prompt: "Explain concepts with a small example.",
      prompt_template: null,
      created_at: "2026-09-18T09:00:00Z",
      updated_at: "2026-09-18T09:00:00Z",
    };
    const state = apiState({ models: [model], loaded: [{ id: model.id, backend: "mock", status: "loaded" }], conversations: [conversation] }) as Record<string, unknown>;
    let completionRequest: Record<string, unknown> | undefined;
    let messageCount = 0;
    const fetchMock = installFetch(state);
    fetchMock.mockImplementation(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input), "http://127.0.0.1:14567").pathname;
      if (path === "/runtime/chat/presets") return jsonResponse([preset]);
      if (path === "/runtime/chat/messages" && init?.method === "POST") {
        const message = JSON.parse(String(init.body)) as { role: string; content: string };
        messageCount += 1;
        return jsonResponse({ id: `message-${messageCount}`, ...message, created_at: "2026-09-18T09:00:00Z" });
      }
      if (path === "/v1/chat/completions") {
        completionRequest = JSON.parse(String(init?.body)) as Record<string, unknown>;
        return jsonResponse({ choices: [{ message: { content: "Example answer." } }] });
      }
      return jsonResponse(state[path] ?? []);
    });
    render(<App />);
    await navigate("Chat");
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Prompt preset" }), preset.id);
    fireEvent.change(screen.getByRole("textbox", { name: "Chat prompt" }), { target: { value: "Explain gravity." } });
    await userEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => expect(completionRequest).toBeDefined());
    expect(completionRequest?.messages).toEqual(expect.arrayContaining([
      expect.objectContaining({ role: "system", content: expect.stringContaining(preset.system_prompt) }),
    ]));
  });
});
