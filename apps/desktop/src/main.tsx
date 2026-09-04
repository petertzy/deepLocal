import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import ReactMarkdown from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";
import remarkGfm from "remark-gfm";
import {
  Activity,
  ArrowLeft,
  ArrowRight,
  Boxes,
  Check,
  Copy,
  Cpu,
  Download,
  FolderOpen,
  Info,
  MessageSquare,
  Pencil,
  Plus,
  Play,
  RefreshCw,
  Search,
  Server,
  Settings,
  Square,
  Trash2,
  X,
} from "lucide-react";
import "./styles.css";

type Tab = "dashboard" | "chat" | "models" | "server" | "settings";
type Health = "online" | "offline";
type SearchSort = "downloads" | "likes" | "smallest-file" | "largest-file" | "name";

type HardwareProfile = {
  os: string;
  arch: string;
  cpu_brand: string;
  cpu_cores: number;
  total_ram_bytes: number;
  available_ram_bytes: number;
  gpu: Array<{ name: string; vendor?: string | null; total_vram_bytes?: number | null }>;
};

type ModelDescriptor = {
  id: string;
  name: string;
  source: string;
  format: string;
  local_path?: string | null;
  size_bytes?: number | null;
  capabilities?: string[];
  files?: Array<{ filename: string; path?: string | null; size_bytes?: number | null; sha256?: string | null }>;
};

type LoadedModel = {
  id: string;
  backend: string;
  status: string;
};

type ChatMessage = {
  id?: string;
  role: "user" | "assistant";
  content: string;
  created_at?: string;
};

type ChatConversation = {
  id: string;
  title: string;
  model_id?: string | null;
  messages: ChatMessage[];
  created_at: string;
  updated_at: string;
};

type HuggingFaceResult = {
  repo: string;
  downloads?: number | null;
  likes?: number | null;
  files: Array<{ filename: string; size_bytes?: number | null }>;
};

type HuggingFaceModelFile = {
  repo: string;
  filename: string;
  size_bytes?: number | null;
  downloads?: number | null;
  likes?: number | null;
};

type DownloadJob = {
  id: string;
  repo: string;
  filename: string;
  status: string;
  downloaded_bytes: number;
  total_bytes?: number | null;
  speed_bytes_per_sec?: number | null;
  eta_seconds?: number | null;
  local_path?: string | null;
  error?: string | null;
  cancel_requested?: boolean;
};

type DiscoveredModelFile = {
  filename: string;
  path: string;
  size_bytes: number;
  suggested_model_id: string;
};

type ModelLoadOptions = {
  context_length: number;
  gpu_layers: number;
};

type StoredModelLoadOptions = Record<string, ModelLoadOptions>;

const API_BASE = "http://127.0.0.1:14567";
const ACTIVE_CHAT_STORAGE_KEY = "deeplocal:active-chat-conversation";
const MODEL_LOAD_OPTIONS_STORAGE_KEY = "deeplocal:model-load-options";

function defaultLoadOptions(hardware: HardwareProfile | null): ModelLoadOptions {
  const isAppleSilicon =
    hardware?.os.toLowerCase().includes("darwin") && hardware?.arch.toLowerCase().includes("arm");
  return {
    context_length: isAppleSilicon ? 4096 : 2048,
    gpu_layers: isAppleSilicon ? -1 : 0,
  };
}

function sanitizeLoadOptions(value: Partial<ModelLoadOptions>, fallback: ModelLoadOptions): ModelLoadOptions {
  const context = Number(value.context_length);
  const gpuLayers = Number(value.gpu_layers);
  return {
    context_length: Number.isFinite(context) && context >= 512 ? Math.round(context) : fallback.context_length,
    gpu_layers: Number.isFinite(gpuLayers) ? Math.round(gpuLayers) : fallback.gpu_layers,
  };
}

function readStoredModelLoadOptions(): StoredModelLoadOptions {
  try {
    const raw = window.localStorage.getItem(MODEL_LOAD_OPTIONS_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, Partial<ModelLoadOptions>>;
    return Object.fromEntries(
      Object.entries(parsed).map(([modelId, options]) => [
        modelId,
        sanitizeLoadOptions(options, { context_length: 4096, gpu_layers: -1 }),
      ]),
    );
  } catch {
    return {};
  }
}

function writeStoredModelLoadOptions(options: StoredModelLoadOptions) {
  window.localStorage.setItem(MODEL_LOAD_OPTIONS_STORAGE_KEY, JSON.stringify(options));
}

function App() {
  const [tab, setTab] = useState<Tab>("dashboard");
  const [health, setHealth] = useState<Health>("offline");
  const [hardware, setHardware] = useState<HardwareProfile | null>(null);
  const [models, setModels] = useState<ModelDescriptor[]>([]);
  const [loaded, setLoaded] = useState<LoadedModel[]>([]);
  const [downloads, setDownloads] = useState<DownloadJob[]>([]);
  const [modelsDirectory, setModelsDirectory] = useState("./models");
  const [hfToken, setHfToken] = useState(() => window.localStorage.getItem("deeplocal:hf-token") ?? "");
  const [notice, setNotice] = useState("Start the API with `cargo run -p deeplocal -- serve`.");

  const refresh = useCallback(async () => {
    try {
      const healthRes = await fetch(`${API_BASE}/health`);
      setHealth(healthRes.ok ? "online" : "offline");
    } catch {
      setHealth("offline");
    }

    try {
      const [hardwareRes, modelsRes, loadedRes, downloadsRes, directoryRes] = await Promise.all([
        fetch(`${API_BASE}/runtime/hardware`),
        fetch(`${API_BASE}/runtime/models`),
        fetch(`${API_BASE}/runtime/models/loaded`),
        fetch(`${API_BASE}/runtime/downloads`),
        fetch(`${API_BASE}/runtime/models/directory`),
      ]);
      if (hardwareRes.ok) setHardware(await hardwareRes.json());
      if (modelsRes.ok) setModels(await modelsRes.json());
      if (loadedRes.ok) setLoaded(await loadedRes.json());
      if (downloadsRes.ok) setDownloads(await downloadsRes.json());
      if (directoryRes.ok) {
        const data = await directoryRes.json();
        setModelsDirectory(data.path);
      }
    } catch {
      setHardware(null);
      setModels([]);
      setLoaded([]);
      setDownloads([]);
    }
  }, []);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, 1500);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const tabs = useMemo(
    () => [
      { id: "dashboard" as const, label: "Dashboard", icon: Activity },
      { id: "chat" as const, label: "Chat", icon: MessageSquare },
      { id: "models" as const, label: "Models", icon: Download },
      { id: "server" as const, label: "Server", icon: Server },
      { id: "settings" as const, label: "Settings", icon: Settings },
    ],
    [],
  );

  return (
    <main className="shell">
      <aside className="sidebar">
        <div className="brand">
          <Cpu size={22} />
          <strong>deepLocal</strong>
        </div>
        <nav>
          {tabs.map((item) => {
            const Icon = item.icon;
            return (
              <button className={tab === item.id ? "active" : ""} key={item.id} onClick={() => setTab(item.id)}>
                <Icon size={18} />
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>
        <div className="sideStatus">
          <span className={`dot ${health}`} />
          <span>{health === "online" ? "Runtime online" : "Runtime offline"}</span>
        </div>
      </aside>

      <section className="workspace">
        <header>
          <div>
            <h1>{tabs.find((item) => item.id === tab)?.label}</h1>
            <p>{notice}</p>
          </div>
          <div className="headerControls">
            <div className={`healthBadge ${health}`}>
              <span className={`dot ${health}`} />
              <span>{health === "online" ? "API online" : "API offline"}</span>
            </div>
            <button className="primary" onClick={refresh}>
              <RefreshCw size={18} />
              Refresh
            </button>
          </div>
        </header>

        {tab === "dashboard" && <Dashboard hardware={hardware} health={health} loaded={loaded} models={models} onOpenModels={() => setTab("models")} />}
        {tab === "chat" && <Chat loaded={loaded} onOpenModels={() => setTab("models")} onNotice={setNotice} />}
        {tab === "models" && (
          <Models
            models={models}
            loaded={loaded}
            downloads={downloads}
            hardware={hardware}
            modelsDirectory={modelsDirectory}
            hfToken={hfToken}
            onNotice={setNotice}
            onRefresh={refresh}
          />
        )}
        {tab === "server" && <ServerPanel hardware={hardware} loaded={loaded} onOpenModels={() => setTab("models")} onNotice={setNotice} />}
        {tab === "settings" && (
          <SettingsPanel modelsDirectory={modelsDirectory} hfToken={hfToken} onTokenChange={setHfToken} onNotice={setNotice} />
        )}
      </section>
    </main>
  );
}

function Dashboard({
  health,
  hardware,
  loaded,
  models,
  onOpenModels,
}: {
  health: Health;
  hardware: HardwareProfile | null;
  loaded: LoadedModel[];
  models: ModelDescriptor[];
  onOpenModels: () => void;
}) {
  const isEmpty = models.length === 0 && loaded.length === 0;

  return (
    <div className="dashboard">
      <Metric title="Runtime" value={health} detail={API_BASE} />
      <Metric title="Registered models" value={models.length.toString()} detail="Runtime catalog" />
      <Metric title="Loaded models" value={loaded.length.toString()} detail={loaded.map((item) => item.id).join(", ") || "None"} />
      <Metric title="Memory" value={formatBytes(hardware?.total_ram_bytes ?? 0)} detail={hardware?.cpu_brand ?? "No hardware profile"} />
      {isEmpty && (
        <EmptyState
          className="wide"
          icon={<Download size={24} />}
          title="No local models yet"
          description="Download a GGUF model or register an existing file to turn this dashboard into a live runtime overview."
          actionLabel="Find models"
          onAction={onOpenModels}
        />
      )}
    </div>
  );
}

function Metric({ title, value, detail }: { title: string; value: string; detail: string }) {
  return (
    <section className="metric">
      <span>{title}</span>
      <strong>{value}</strong>
      <p>{detail}</p>
    </section>
  );
}

function ProgressItem({ done, title, detail }: { done?: boolean; title: string; detail: string }) {
  return (
    <div className="progressItem">
      <span className={done ? "check done" : "check"}>{done ? "✓" : ""}</span>
      <div>
        <strong>{title}</strong>
        <p>{detail}</p>
      </div>
    </div>
  );
}

function Chat({ loaded, onOpenModels, onNotice }: { loaded: LoadedModel[]; onOpenModels: () => void; onNotice: (message: string) => void }) {
  const [input, setInput] = useState("Could you please introduce yourself in detail? Thank you.");
  const [conversations, setConversations] = useState<ChatConversation[]>([]);
  const [activeConversationId, setActiveConversationId] = useState(() => window.localStorage.getItem(ACTIVE_CHAT_STORAGE_KEY) ?? "");
  const [showConversationList, setShowConversationList] = useState(false);
  const [streaming, setStreaming] = useState(true);
  const [isGenerating, setIsGenerating] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const transcriptRef = useRef<HTMLDivElement | null>(null);
  const activeModel = loaded.find((model) => model.backend !== "mock")?.id ?? loaded[0]?.id;
  const activeConversation = conversations.find((conversation) => conversation.id === activeConversationId) ?? conversations[0];
  const conversationModel = activeConversation?.model_id ?? activeModel;
  const messages = activeConversation?.messages ?? [];

  const refreshConversations = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/runtime/chat/conversations`);
      if (!res.ok) throw new Error(await res.text());
      const items: ChatConversation[] = await res.json();
      setConversations(items);
      setActiveConversationId((current) => {
        const next = items.some((conversation) => conversation.id === current) ? current : (items[0]?.id ?? "");
        if (next) window.localStorage.setItem(ACTIVE_CHAT_STORAGE_KEY, next);
        else window.localStorage.removeItem(ACTIVE_CHAT_STORAGE_KEY);
        return next;
      });
    } catch {
      setConversations([]);
    }
  }, []);

  useEffect(() => {
    refreshConversations();
  }, [refreshConversations]);

  useEffect(() => {
    const transcript = transcriptRef.current;
    if (transcript) transcript.scrollTop = transcript.scrollHeight;
  }, [activeConversationId, messages.length, messages.at(-1)?.content]);

  function selectConversation(id: string) {
    setActiveConversationId(id);
    window.localStorage.setItem(ACTIVE_CHAT_STORAGE_KEY, id);
    setShowConversationList(false);
  }

  async function send() {
    const prompt = input.trim();
    if (!prompt || isGenerating) return;
    if (!conversationModel) {
      onNotice("Load a model before chatting.");
      return;
    }

    setInput("");
    setIsGenerating(true);
    try {
      const conversation = activeConversation ?? (await createConversation(titleFromPrompt(prompt), conversationModel));
      selectConversation(conversation.id);
      if (!conversation.model_id) {
        await updateConversationModel(conversation.id, conversationModel);
      }
      const userMessage = await appendConversationMessage(conversation.id, "user", prompt);
      const nextMessages: ChatMessage[] = [...conversation.messages, userMessage];
      updateConversationMessages(conversation.id, nextMessages, conversation.model_id ?? conversationModel);

      if (streaming) {
        const assistantDraft: ChatMessage = {
          id: `streaming-${Date.now()}`,
          role: "assistant",
          content: "",
          created_at: new Date().toISOString(),
        };
        updateConversationMessages(conversation.id, [...nextMessages, assistantDraft], conversation.model_id ?? conversationModel);

        const controller = new AbortController();
        abortRef.current = controller;
        let content = "";
        let stopped = false;
        try {
          await streamChatCompletion(
            conversation.model_id ?? conversationModel,
            nextMessages,
            controller.signal,
            (token) => {
              content += token;
              updateConversationMessages(
                conversation.id,
                [...nextMessages, { ...assistantDraft, content }],
                conversation.model_id ?? conversationModel,
              );
            },
          );
        } catch (error) {
          if (error instanceof DOMException && error.name === "AbortError") stopped = true;
          else throw error;
        } finally {
          abortRef.current = null;
        }

        if (content.trim()) {
          const assistantMessage = await appendConversationMessage(conversation.id, "assistant", content);
          updateConversationMessages(conversation.id, [...nextMessages, assistantMessage], conversation.model_id ?? conversationModel);
        } else {
          updateConversationMessages(conversation.id, nextMessages, conversation.model_id ?? conversationModel);
        }
        onNotice(stopped ? "Chat generation stopped." : `Chat completed with ${conversation.model_id ?? conversationModel}.`);
        return;
      }

      const res = await fetch(`${API_BASE}/v1/chat/completions`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          model: conversation.model_id ?? conversationModel,
          stream: false,
          messages: nextMessages,
        }),
      });
      const data = await res.json();
      const content = data.choices?.[0]?.message?.content ?? data.error ?? "No response";
      const assistantMessage = await appendConversationMessage(conversation.id, "assistant", content);
      updateConversationMessages(conversation.id, [...nextMessages, assistantMessage], conversation.model_id ?? conversationModel);
      onNotice(`Chat completed with ${conversation.model_id ?? conversationModel}.`);
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") onNotice("Chat generation stopped.");
      else {
        onNotice("Chat request failed. Start the deepLocal API and load a model.");
        refreshConversations();
      }
    } finally {
      abortRef.current = null;
      setIsGenerating(false);
    }
  }

  function stopGeneration() {
    abortRef.current?.abort();
  }

  async function createConversation(title = "New conversation", modelId = activeModel ?? null) {
    const res = await fetch(`${API_BASE}/runtime/chat/conversations`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ title, model_id: modelId }),
    });
    if (!res.ok) throw new Error(await res.text());
    const conversation: ChatConversation = await res.json();
    setConversations((items) => [conversation, ...items]);
    selectConversation(conversation.id);
    setShowConversationList(false);
    onNotice("Conversation created.");
    return conversation;
  }

  async function renameConversation(conversation: ChatConversation) {
    const title = window.prompt("Rename conversation", conversation.title)?.trim();
    if (!title) return;
    const res = await fetch(`${API_BASE}/runtime/chat/conversations/rename`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id: conversation.id, title }),
    });
    if (!res.ok) {
      onNotice(await res.text());
      return;
    }
    setConversations((items) => items.map((item) => (item.id === conversation.id ? { ...item, title } : item)));
    onNotice("Conversation renamed.");
  }

  async function deleteConversation(conversation: ChatConversation) {
    if (!window.confirm(`Delete "${conversation.title}"?`)) return;
    const res = await fetch(`${API_BASE}/runtime/chat/conversations/delete`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id: conversation.id }),
    });
    if (!res.ok) {
      onNotice(await res.text());
      return;
    }
    setConversations((items) => items.filter((item) => item.id !== conversation.id));
    if (conversation.id === activeConversationId) {
      window.localStorage.removeItem(ACTIVE_CHAT_STORAGE_KEY);
      setActiveConversationId("");
    }
    onNotice("Conversation deleted.");
    refreshConversations();
  }

  return (
    <div className={showConversationList ? "pane chatLayout showingConversations" : "pane chatLayout"}>
      {showConversationList ? (
      <section className="conversationList">
        <div className="conversationHeader">
          <button className="iconButton" title="Back to current chat" onClick={() => setShowConversationList(false)}>
            <ArrowRight size={16} />
          </button>
          <strong>Conversations</strong>
          <button className="iconButton" title="New conversation" onClick={() => createConversation()}>
            <Plus size={16} />
          </button>
        </div>
        <div className="conversationItems">
          {conversations.length ? (
            conversations.map((conversation) => (
              <button
                className={conversation.id === activeConversation?.id ? "conversationItem active" : "conversationItem"}
                key={conversation.id}
                onClick={() => selectConversation(conversation.id)}
              >
                <span>{conversation.title}</span>
                <em>{conversationPreview(conversation)}</em>
                <small>
                  {formatConversationTime(conversation.updated_at)} · {conversation.model_id ?? "No model yet"}
                </small>
              </button>
            ))
          ) : (
            <p>No conversations yet</p>
          )}
        </div>
      </section>
      ) : (
      <div className="chat">
        <div className="chatHeader">
          <button className="iconButton" title="Show conversations" onClick={() => setShowConversationList(true)}>
            <ArrowLeft size={16} />
          </button>
          <div className="chatTitle">
            <h2>{activeConversation?.title ?? "Chat"}</h2>
            <span title={conversationModel ?? "No model loaded"}>
              <Boxes size={16} />
              {conversationModel ?? "No model loaded"}
            </span>
          </div>
          <div className="chatActions">
            <button className="iconButton" disabled={!activeConversation} title="Rename conversation" onClick={() => activeConversation && renameConversation(activeConversation)}>
              <Pencil size={16} />
            </button>
            <button className="iconButton" disabled={!activeConversation} title="Delete conversation" onClick={() => activeConversation && deleteConversation(activeConversation)}>
              <Trash2 size={16} />
            </button>
          </div>
        </div>
        <div className="transcript" ref={transcriptRef}>
          {!messages.length && !conversationModel ? (
            <EmptyState
              icon={<MessageSquare size={24} />}
              title="Load a model to start chatting"
              description="Choose a downloaded GGUF model first, then send prompts here through the local API."
              actionLabel="Open models"
              onAction={onOpenModels}
            />
          ) : !activeConversation ? (
            <EmptyState
              icon={<MessageSquare size={24} />}
              title="Start a conversation"
              description="Create a conversation or send a prompt to begin a saved local chat."
              actionLabel="New conversation"
              onAction={() => createConversation()}
            />
          ) : (
            messages.map((message, index) => (
              <div className={`message ${message.role}`} key={message.id ?? `${message.role}-${index}`}>
                {message.role === "assistant" ? (
                  <ReactMarkdown
                    remarkPlugins={[remarkGfm]}
                    components={{
                      code({ className, children, ...props }) {
                        const language = /language-(\w+)/.exec(className ?? "")?.[1];
                        const code = String(children).replace(/\n$/, "");

                        if (language) {
                          return (
                            <SyntaxHighlighter
                              PreTag="div"
                              className="codeBlock"
                              language={language}
                              style={oneLight}
                              customStyle={{
                                margin: 0,
                                padding: 0,
                                background: "transparent",
                              }}
                              codeTagProps={{
                                style: {
                                  background: "transparent",
                                  fontFamily: "inherit",
                                },
                              }}
                            >
                              {code}
                            </SyntaxHighlighter>
                          );
                        }

                        return (
                          <code className={className} {...props}>
                            {children}
                          </code>
                        );
                      },
                    }}
                  >
                    {normalizeMarkdown(message.content)}
                  </ReactMarkdown>
                ) : (
                  message.content
                )}
              </div>
            ))
          )}
        </div>
        <div className="composer">
          <label className="streamToggle">
            <input type="checkbox" checked={streaming} disabled={isGenerating} onChange={(event) => setStreaming(event.target.checked)} />
            <span>Streaming</span>
          </label>
          <input value={input} onChange={(event) => setInput(event.target.value)} onKeyDown={(event) => event.key === "Enter" && send()} />
          {isGenerating ? (
            <button onClick={stopGeneration}>
              <Square size={16} />
              Stop
            </button>
          ) : (
            <button disabled={!conversationModel} onClick={send}>
              Send
            </button>
          )}
        </div>
      </div>
      )}
    </div>
  );

  async function updateConversationModel(id: string, modelId: string) {
    const res = await fetch(`${API_BASE}/runtime/chat/conversations/model`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id, model_id: modelId }),
    });
    if (!res.ok) throw new Error(await res.text());
  }

  async function appendConversationMessage(sessionId: string, role: ChatMessage["role"], content: string) {
    const res = await fetch(`${API_BASE}/runtime/chat/messages`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ session_id: sessionId, role, content }),
    });
    if (!res.ok) throw new Error(await res.text());
    return (await res.json()) as ChatMessage;
  }

  function updateConversationMessages(id: string, nextMessages: ChatMessage[], modelId: string) {
    setConversations((items) =>
      items.map((conversation) =>
        conversation.id === id
          ? { ...conversation, model_id: modelId, messages: nextMessages, updated_at: new Date().toISOString() }
          : conversation,
      ),
    );
  }
}

function Models({
  models,
  loaded,
  downloads,
  hardware,
  modelsDirectory,
  hfToken,
  onNotice,
  onRefresh,
}: {
  models: ModelDescriptor[];
  loaded: LoadedModel[];
  downloads: DownloadJob[];
  hardware: HardwareProfile | null;
  modelsDirectory: string;
  hfToken: string;
  onNotice: (message: string) => void;
  onRefresh: () => Promise<void>;
}) {
  const [id, setId] = useState("");
  const [path, setPath] = useState("");
  const [query, setQuery] = useState("Gemma 3 1b");
  const [results, setResults] = useState<HuggingFaceResult[]>([]);
  const [sortBy, setSortBy] = useState<SearchSort>("downloads");
  const [searching, setSearching] = useState(false);
  const [pendingDownloads, setPendingDownloads] = useState<Record<string, DownloadJob>>({});
  const [detailsModelId, setDetailsModelId] = useState<string | null>(null);
  const [loadOptionsByModel, setLoadOptionsByModel] = useState<StoredModelLoadOptions>(() => readStoredModelLoadOptions());
  const [discoveredFiles, setDiscoveredFiles] = useState<DiscoveredModelFile[]>([]);
  const [rescanning, setRescanning] = useState(false);
  const fallbackLoadOptions = useMemo(() => defaultLoadOptions(hardware), [hardware]);

  const downloadByFile = useMemo(() => {
    const items = new Map<string, DownloadJob>();
    for (const job of downloads.filter((item) => isActiveDownload(item.status))) {
      items.set(downloadKey(job.repo, job.filename), job);
    }
    return items;
  }, [downloads]);
  const downloadHistory = useMemo(() => downloads.filter((job) => isDownloadHistory(job.status)), [downloads]);
  const canRegisterManualModel = !!id.trim() && !!path.trim();

  useEffect(() => {
    setPendingDownloads((current) => {
      const next = { ...current };
      for (const key of Object.keys(next)) {
        if (downloadByFile.has(key)) delete next[key];
      }
      return next;
    });
  }, [downloadByFile]);

  async function register() {
    if (!id.trim() || !path.trim()) {
      onNotice("Enter a model id and GGUF file path before registering.");
      return;
    }
    const descriptor = createLocalDescriptor(id, path);
    const res = await fetch(`${API_BASE}/runtime/models`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(descriptor),
    });
    onNotice(res.ok ? `Registered ${id}.` : await res.text());
    await onRefresh();
  }

  function updateManualPath(value: string) {
    setPath(value);
    if (!id.trim()) {
      setId(modelIdFromPath(value));
    }
  }

  async function registerDiscoveredFile(file: DiscoveredModelFile) {
    const descriptor = createLocalDescriptor(file.suggested_model_id, file.path);
    descriptor.name = file.suggested_model_id;
    descriptor.size_bytes = file.size_bytes;
    const res = await fetch(`${API_BASE}/runtime/models`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(descriptor),
    });
    if (res.ok) {
      onNotice(`Registered ${file.suggested_model_id}.`);
      setDiscoveredFiles((current) => current.filter((item) => item.path !== file.path));
      await onRefresh();
    } else {
      onNotice(await res.text());
      await rescanModels();
    }
  }

  async function rescanModels() {
    setRescanning(true);
    try {
      const res = await fetch(`${API_BASE}/runtime/models/rescan`, { method: "POST" });
      if (!res.ok) throw new Error(await res.text());
      const data = (await res.json()) as { files: DiscoveredModelFile[] };
      setDiscoveredFiles(data.files);
      onNotice(data.files.length ? `Found ${data.files.length} unregistered GGUF files.` : "No unregistered GGUF files found.");
    } catch (error) {
      setDiscoveredFiles([]);
      onNotice(error instanceof Error ? error.message : "Model rescan failed.");
    } finally {
      setRescanning(false);
    }
  }

  async function load(modelId: string) {
    const options = loadOptionsForModel(modelId);
    const res = await fetch(`${API_BASE}/runtime/models/load`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ model_id: modelId, backend: "llama.cpp", ...options }),
    });
    onNotice(res.ok ? `Loaded ${modelId}.` : await res.text());
    await onRefresh();
  }

  function loadOptionsForModel(modelId: string) {
    return sanitizeLoadOptions(loadOptionsByModel[modelId] ?? fallbackLoadOptions, fallbackLoadOptions);
  }

  function updateLoadOption(modelId: string, field: keyof ModelLoadOptions, value: string) {
    const numericValue = Number(value);
    setLoadOptionsByModel((current) => {
      const currentOptions = sanitizeLoadOptions(current[modelId] ?? fallbackLoadOptions, fallbackLoadOptions);
      const nextOptions = sanitizeLoadOptions(
        { ...currentOptions, [field]: numericValue },
        fallbackLoadOptions,
      );
      const next = { ...current, [modelId]: nextOptions };
      writeStoredModelLoadOptions(next);
      return next;
    });
  }

  async function unload(modelId: string) {
    const res = await fetch(`${API_BASE}/runtime/models/unload`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ model_id: modelId }),
    });
    onNotice(res.ok ? `Unloaded ${modelId}.` : `Failed to unload ${modelId}.`);
    await onRefresh();
  }

  async function searchHuggingFace() {
    const term = query.trim();
    if (!term) return;
    setSearching(true);
    onNotice(`Searching Hugging Face for ${term}...`);
    try {
      const res = await fetch(`${API_BASE}/runtime/huggingface/search?query=${encodeURIComponent(term)}&limit=12`, {
        headers: huggingFaceHeaders(hfToken),
      });
      if (!res.ok) throw new Error(await res.text());
      const data = await res.json();
      setResults(data);
      onNotice(`Found ${data.length} GGUF repositories.`);
    } catch (error) {
      setResults([]);
      onNotice(error instanceof Error ? error.message : "Hugging Face search failed.");
    } finally {
      setSearching(false);
    }
  }

  async function downloadFile(repo: string, filename: string, sizeBytes?: number | null) {
    const modelId = `${repo}:${filename}`;
    const key = downloadKey(repo, filename);
    setPendingDownloads((current) => ({
      ...current,
      [key]: {
        id: key,
        repo,
        filename,
        status: "starting",
        downloaded_bytes: 0,
        total_bytes: sizeBytes ?? null,
        speed_bytes_per_sec: null,
        eta_seconds: null,
      },
    }));
    const res = await fetch(`${API_BASE}/runtime/huggingface/download`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ repo, filename, model_id: modelId, size_bytes: sizeBytes ?? null, token: hfToken || null }),
    });
    if (res.ok) {
      onNotice(`Download started for ${filename}.`);
    } else {
      const message = await res.text();
      setPendingDownloads((current) => ({
        ...current,
        [key]: {
          id: key,
          repo,
          filename,
          status: "error",
          downloaded_bytes: 0,
          total_bytes: sizeBytes ?? null,
          speed_bytes_per_sec: null,
          eta_seconds: null,
          error: message || `Failed to start download for ${filename}.`,
        },
      }));
      onNotice(message || `Failed to start download for ${filename}.`);
    }
    await onRefresh();
  }

  async function cancelDownload(job: DownloadJob) {
    const key = downloadKey(job.repo, job.filename);
    setPendingDownloads((current) => ({
      ...current,
      [key]: { ...job, status: "cancelling", cancel_requested: true },
    }));
    const res = await fetch(`${API_BASE}/runtime/downloads/cancel`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ job_id: job.id, repo: job.repo, filename: job.filename }),
    });
    if (res.ok) {
      onNotice(`Stopping download for ${job.filename}.`);
    } else {
      const message = await res.text();
      onNotice(message || `Failed to stop download for ${job.filename}.`);
    }
    await onRefresh();
  }

  async function discardDownload(job: DownloadJob) {
    if (!window.confirm(`Discard download for ${job.filename} and delete its partial file?`)) return;
    const key = downloadKey(job.repo, job.filename);
    setPendingDownloads((current) => {
      const next = { ...current };
      delete next[key];
      return next;
    });
    const res = await fetch(`${API_BASE}/runtime/downloads/discard`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ job_id: job.id, repo: job.repo, filename: job.filename }),
    });
    onNotice(res.ok ? `Discarded download for ${job.filename}.` : await res.text());
    await onRefresh();
  }

  async function clearDownloadHistory() {
    const res = await fetch(`${API_BASE}/runtime/downloads/clear-history`, { method: "POST" });
    if (res.ok) {
      const data = await res.json();
      onNotice(`Cleared ${data.cleared ?? 0} download history items.`);
    } else {
      onNotice("Failed to clear download history.");
    }
    await onRefresh();
  }

  async function openModelsDirectory() {
    const res = await fetch(`${API_BASE}/runtime/models/open-directory`, { method: "POST" });
    onNotice(res.ok ? `Opened ${modelsDirectory}.` : `Failed to open ${modelsDirectory}.`);
  }

  async function copyModelPath(model: ModelDescriptor, modelPath: string | null) {
    if (!modelPath) {
      onNotice(`No local path registered for ${model.name}.`);
      return false;
    }
    try {
      await navigator.clipboard.writeText(modelPath);
      onNotice(`Copied path for ${model.name}.`);
      return true;
    } catch {
      onNotice(`Copy failed. Path: ${modelPath}`);
      return false;
    }
  }

  async function revealModel(model: ModelDescriptor, modelPath: string | null) {
    if (!modelPath) {
      onNotice(`No local path registered for ${model.name}.`);
      return;
    }
    const res = await fetch(`${API_BASE}/runtime/models/reveal`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ path: modelPath }),
    });
    onNotice(res.ok ? `Revealed ${model.name}.` : `Failed to reveal ${model.name}.`);
  }

  async function deleteModel(model: ModelDescriptor, modelPath: string | null) {
    if (!window.confirm(`Delete ${model.name} and remove its local file?\n\n${modelPath ?? "No local path registered"}`)) return;
    const res = await fetch(`${API_BASE}/runtime/models/delete`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ model_id: model.id, delete_file: true }),
    });
    if (res.ok) {
      setDetailsModelId(null);
      onNotice(`Deleted ${model.name}.`);
      await onRefresh();
    } else {
      onNotice(await res.text());
    }
  }

  const detailsModel = models.find((model) => model.id === detailsModelId) ?? null;
  const detailsHandle = detailsModel ? loaded.find((item) => item.id === detailsModel.id) ?? null : null;
  const detailsPath = detailsModel ? resolveModelPath(detailsModel.local_path, modelsDirectory) : null;
  const sortedFiles = useMemo(() => sortSearchFiles(flattenSearchResults(results), sortBy), [results, sortBy]);

  return (
    <div className="pane">
      <div className="paneHeader">
        <h2>Model lifecycle</h2>
        <div className="headerActions">
          <button onClick={openModelsDirectory}>
            <FolderOpen size={16} />
            Open Folder
          </button>
          <button onClick={onRefresh}>
            <RefreshCw size={16} />
            Refresh
          </button>
        </div>
      </div>
      <div className="folderPath">{modelsDirectory}</div>
      <section className="discoveredPanel">
        <div className="sectionTitle">
          <h2>Local GGUF discovery</h2>
          <span>{rescanning ? "scanning" : `${discoveredFiles.length} unregistered`}</span>
        </div>
        <div className="discoveryToolbar">
          <p>Scan the models folder for GGUF files that are not registered yet.</p>
          <button onClick={rescanModels} disabled={rescanning}>
            <RefreshCw size={16} />
            Rescan
          </button>
        </div>
        {discoveredFiles.length ? (
          <div className="discoveredList">
            {discoveredFiles.map((file) => (
              <div className="discoveredRow" key={file.path}>
                <div>
                  <strong>{file.filename}</strong>
                  <p>{file.path}</p>
                </div>
                <span>{formatFileSize(file.size_bytes)}</span>
                <code>{file.suggested_model_id}</code>
                <button onClick={() => registerDiscoveredFile(file)}>
                  <Plus size={15} />
                  Register
                </button>
              </div>
            ))}
          </div>
        ) : (
          <EmptyState
            compact
            icon={<FolderOpen size={22} />}
            title="No unregistered local files"
            description="Run a rescan after adding GGUF files to the models folder."
          />
        )}
      </section>
      <section className="discoverPanel">
        <div className="sectionTitle">
          <h2>Hugging Face GGUF search</h2>
          <span>{searching ? "searching" : `${sortedFiles.length} files`}</span>
        </div>
        <div className="searchRow">
          <input value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => event.key === "Enter" && searchHuggingFace()} />
          <label className="sortControl">
            <span>Sort</span>
            <select value={sortBy} onChange={(event) => setSortBy(event.target.value as SearchSort)}>
              <option value="downloads">Repo downloads (source)</option>
              <option value="likes">Repo likes (source)</option>
              <option value="smallest-file">Smallest file</option>
              <option value="largest-file">Largest file</option>
              <option value="name">Name</option>
            </select>
          </label>
          <button onClick={searchHuggingFace}>
            <Download size={16} />
            Search
          </button>
        </div>
        <div className="searchResults">
          {!results.length && (
            <EmptyState
              compact
              icon={<Search size={22} />}
              title={searching ? "Searching model repositories" : "Search for a GGUF model"}
              description={searching ? "Results will appear here as soon as Hugging Face responds." : "Try a small instruct model first, then download the file that fits your hardware."}
            />
          )}
          {sortedFiles.map((file) => {
            const key = downloadKey(file.repo, file.filename);
            const job = downloadByFile.get(key) ?? pendingDownloads[key];
            const canCancel = !!job && ["queued", "starting", "downloading", "cancelling"].includes(job.status);
            return (
              <article className="searchModelCard" key={`${file.repo}-${file.filename}`}>
                <div>
                  <h2>{file.filename}</h2>
                  <p>{file.repo}</p>
                </div>
                <div className="modelFileMeta">
                  <span>{formatFileSize(file.size_bytes)}</span>
                  <span>{modelQuantizationLabel(file.filename)}</span>
                  <span>{file.downloads ?? 0} source downloads</span>
                  <span>{file.likes ?? 0} source likes</span>
                </div>
                {job ? (
                  <div className="inlineDownload">
                    <span className={`jobStatus ${job.status}`}>{downloadStatusLabel(job.status)}</span>
                    <progress value={job.downloaded_bytes} max={job.total_bytes ?? (job.downloaded_bytes || 1)} />
                    <em>
                      {downloadPercent(job)} / {formatTransferredSize(job.downloaded_bytes)} of {formatFileSize(job.total_bytes)}
                    </em>
                    <em>{downloadSpeedAndEta(job)}</em>
                    {job.error && <p>{job.error}</p>}
                  </div>
                ) : null}
                <div className="downloadActions">
                  {job && canCancel ? (
                    <>
                      <button disabled={job.status === "cancelling"} onClick={() => cancelDownload(job)}>
                        <Square size={15} />
                        {downloadActionLabel(job)}
                      </button>
                      <button className="dangerAction" onClick={() => discardDownload(job)}>
                        <Trash2 size={15} />
                        Discard
                      </button>
                    </>
                  ) : (
                    <button
                      disabled={job?.status === "downloaded"}
                      onClick={() => downloadFile(file.repo, file.filename, file.size_bytes)}
                    >
                      <Download size={15} />
                      {job ? downloadActionLabel(job) : "Download"}
                    </button>
                  )}
                </div>
            </article>
            );
          })}
        </div>
      </section>
      <section className="downloadHistory">
        <div className="sectionTitle">
          <h2>Download history</h2>
          <span>{downloadHistory.length} items</span>
        </div>
        {downloadHistory.length ? (
          <>
            <div className="historyList">
              {downloadHistory.map((job) => (
                <div className="historyRow" key={job.id}>
                  <div>
                    <strong>{job.filename}</strong>
                    <p>{job.repo}</p>
                    {job.error && <p>{job.error}</p>}
                  </div>
                  <span className={`jobStatus ${job.status}`}>{downloadStatusLabel(job.status)}</span>
                  <em>{formatTransferredSize(job.downloaded_bytes)} of {formatFileSize(job.total_bytes)}</em>
                  <div className="downloadActions">
                    {job.status !== "downloaded" && (
                      <>
                        <button onClick={() => downloadFile(job.repo, job.filename, job.total_bytes)}>
                          <Download size={15} />
                          Retry
                        </button>
                        <button className="dangerAction" onClick={() => discardDownload(job)}>
                          <Trash2 size={15} />
                          Discard
                        </button>
                      </>
                    )}
                  </div>
                </div>
              ))}
            </div>
            <button className="secondaryAction" onClick={clearDownloadHistory}>Clear history</button>
          </>
        ) : (
          <EmptyState
            compact
            icon={<Download size={22} />}
            title="No download history"
            description="Completed, cancelled, and failed downloads will appear here."
          />
        )}
      </section>
      <section className="manualRegistration">
        <div className="sectionTitle">
          <h2>Manual registration</h2>
          <span>existing local file</span>
        </div>
        <div className="modelTools">
          <label>
            <span>Model ID</span>
            <input
              placeholder="gemma-3-1b-local"
              value={id}
              onChange={(event) => setId(event.target.value)}
            />
          </label>
          <label>
            <span>GGUF file path</span>
            <input
              placeholder={`${modelsDirectory}/model.gguf`}
              value={path}
              onChange={(event) => updateManualPath(event.target.value)}
            />
          </label>
          <button disabled={!canRegisterManualModel} onClick={register}>
            <Plus size={15} />
            Register
          </button>
        </div>
      </section>
      <div className="registeredModelList">
        {models.map((model) => {
          const handle = loaded.find((item) => item.id === model.id);
          const modelPath = resolveModelPath(model.local_path, modelsDirectory);
          const loadOptions = loadOptionsForModel(model.id);
          return (
            <article className="registeredModelRow" key={model.id}>
              <h2>{model.name}</h2>
              <p>
                {model.source} / {model.format}
              </p>
              <div className="modelPath">
                <span>{modelPath ?? "No local path registered"}</span>
                <CopyButton disabled={!modelPath} onCopy={() => copyModelPath(model, modelPath)} />
              </div>
              <ModelLoadOptionsFields
                disabled={!!handle}
                options={loadOptions}
                onChange={(field, value) => updateLoadOption(model.id, field, value)}
              />
              <div className="actions">
                {handle ? (
                  <button onClick={() => unload(model.id)}>
                    <Square size={15} />
                    Unload
                  </button>
                ) : (
                  <button onClick={() => load(model.id)}>
                    <Play size={15} />
                    Load
                  </button>
                )}
                <span>{handle?.status ?? "registered"}</span>
                <button className="secondaryAction" onClick={() => setDetailsModelId(model.id)}>
                  <Info size={15} />
                  Details
                </button>
                <button className="dangerAction" disabled={!!handle} title={handle ? "Unload the model before deleting it" : undefined} onClick={() => deleteModel(model, modelPath)}>
                  <Trash2 size={15} />
                  Delete
                </button>
              </div>
            </article>
          );
        })}
        {!models.length && (
          <EmptyState
            icon={<Boxes size={24} />}
            title="No registered models"
            description="Downloaded and manually registered GGUF files will appear here, ready to load or unload."
            actionLabel="Search Hugging Face"
            onAction={searchHuggingFace}
          />
        )}
      </div>
      {detailsModel && (
        <ModelDetailsDrawer
          model={detailsModel}
          modelPath={detailsPath}
          handle={detailsHandle}
          loadOptions={loadOptionsForModel(detailsModel.id)}
          onClose={() => setDetailsModelId(null)}
          onCopyPath={() => copyModelPath(detailsModel, detailsPath)}
          onLoadOptionChange={(field, value) => updateLoadOption(detailsModel.id, field, value)}
          onLoad={() => load(detailsModel.id)}
          onUnload={() => unload(detailsModel.id)}
          onReveal={() => revealModel(detailsModel, detailsPath)}
          onDelete={() => deleteModel(detailsModel, detailsPath)}
        />
      )}
    </div>
  );
}

function ModelLoadOptionsFields({
  disabled,
  options,
  onChange,
}: {
  disabled: boolean;
  options: ModelLoadOptions;
  onChange: (field: keyof ModelLoadOptions, value: string) => void;
}) {
  return (
    <div className="loadOptions">
      <label>
        <span>Context length</span>
        <input
          disabled={disabled}
          inputMode="numeric"
          min={512}
          step={512}
          type="number"
          value={options.context_length}
          onChange={(event) => onChange("context_length", event.target.value)}
        />
      </label>
      <label>
        <span>GPU layers</span>
        <input
          disabled={disabled}
          inputMode="numeric"
          step={1}
          type="number"
          value={options.gpu_layers}
          onChange={(event) => onChange("gpu_layers", event.target.value)}
        />
      </label>
    </div>
  );
}

function ServerPanel({
  hardware,
  loaded,
  onOpenModels,
  onNotice,
}: {
  hardware: HardwareProfile | null;
  loaded: LoadedModel[];
  onOpenModels: () => void;
  onNotice: (message: string) => void;
}) {
  const baseUrl = `${API_BASE}/v1`;
  const modelsUrl = `${API_BASE}/v1/models`;
  const chatUrl = `${API_BASE}/v1/chat/completions`;
  const activeModel = loaded.find((model) => model.backend !== "mock")?.id ?? loaded[0]?.id;
  const curlExample = `curl ${chatUrl} \\
  -H 'content-type: application/json' \\
  -d '{
    "model": "${activeModel ?? "your-loaded-model-id"}",
    "messages": [
      { "role": "user", "content": "hello" }
    ]
  }'`;

  async function copyText(label: string, value: string) {
    try {
      await navigator.clipboard.writeText(value);
      onNotice(`Copied ${label}.`);
      return true;
    } catch {
      onNotice(`Copy failed. ${label}: ${value}`);
      return false;
    }
  }

  return (
    <div className="pane serverPane">
      <div className="sectionTitle">
        <h2>deepLocal API</h2>
        <span>{loaded.length ? `${loaded.length} loaded` : "no model loaded"}</span>
      </div>
      <div className="apiGrid">
        <ApiEndpoint title="Base URL" value={baseUrl} onCopy={() => copyText("Base URL", baseUrl)} />
        <ApiEndpoint title="Models" value={modelsUrl} onCopy={() => copyText("Models URL", modelsUrl)} />
        <ApiEndpoint title="Chat Completions" value={chatUrl} onCopy={() => copyText("Chat Completions URL", chatUrl)} />
        <ApiEndpoint title="Model ID" value={activeModel ?? "Load a model first"} disabled={!activeModel} onCopy={() => (activeModel ? copyText("Model ID", activeModel) : false)} />
      </div>
      {!activeModel && (
        <EmptyState
          compact
          icon={<Server size={24} />}
          title="API is ready for a model"
          description="Load a GGUF model to make chat completions available from local clients."
          actionLabel="Load model"
          onAction={onOpenModels}
        />
      )}
      <div className="metrics">
        <div>
          <strong>{hardware?.cpu_brand ?? "Unknown CPU"}</strong>
          <span>
            {hardware?.os ?? "Unknown OS"} / {hardware?.arch ?? "unknown"}
          </span>
        </div>
        <div>
          <strong>{hardware?.cpu_cores ?? 0}</strong>
          <span>CPU cores</span>
        </div>
        <div>
          <strong>{formatBytes(hardware?.total_ram_bytes ?? 0)}</strong>
          <span>Total RAM</span>
        </div>
        <div>
          <strong>{loaded.length}</strong>
          <span>Loaded models</span>
        </div>
      </div>
      <div className="codeHeader">
        <h2>curl</h2>
        <CopyButton onCopy={() => copyText("curl example", curlExample)} />
      </div>
      <pre>{curlExample}</pre>
    </div>
  );
}

function EmptyState({
  icon,
  title,
  description,
  actionLabel,
  compact,
  className,
  onAction,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  actionLabel?: string;
  compact?: boolean;
  className?: string;
  onAction?: () => void;
}) {
  return (
    <section className={`emptyState ${compact ? "compact" : ""} ${className ?? ""}`.trim()}>
      <div className="emptyIcon">{icon}</div>
      <div>
        <h2>{title}</h2>
        <p>{description}</p>
      </div>
      {actionLabel && onAction && (
        <button className="primary" onClick={onAction}>
          <ArrowRight size={16} />
          {actionLabel}
        </button>
      )}
    </section>
  );
}

function ModelDetailsDrawer({
  model,
  modelPath,
  handle,
  loadOptions,
  onClose,
  onCopyPath,
  onLoadOptionChange,
  onLoad,
  onUnload,
  onReveal,
  onDelete,
}: {
  model: ModelDescriptor;
  modelPath: string | null;
  handle: LoadedModel | null;
  loadOptions: ModelLoadOptions;
  onClose: () => void;
  onCopyPath: () => Promise<boolean>;
  onLoadOptionChange: (field: keyof ModelLoadOptions, value: string) => void;
  onLoad: () => Promise<void>;
  onUnload: () => Promise<void>;
  onReveal: () => Promise<void>;
  onDelete: () => Promise<void>;
}) {
  useEffect(() => {
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  const capabilities = model.capabilities?.length ? model.capabilities.join(", ") : "None listed";
  const loadState = handle ? handle.status : "Not loaded";

  return (
    <div className="drawerLayer" role="presentation" onMouseDown={onClose}>
      <aside
        className="detailsDrawer"
        role="dialog"
        aria-modal="true"
        aria-labelledby="model-details-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="drawerHeader">
          <div>
            <span>Model details</span>
            <h2 id="model-details-title">{model.name}</h2>
          </div>
          <button className="iconButton" aria-label="Close model details" autoFocus onClick={onClose}>
            <X size={18} />
          </button>
        </div>

        <div className="detailList">
          <DetailItem label="Model ID" value={model.id} />
          <DetailItem label="Source" value={model.source} />
          <DetailItem label="Format" value={String(model.format)} />
          <DetailItem label="Path" value={modelPath ?? "No local path registered"} />
          <DetailItem label="Size" value={formatFileSize(model.size_bytes ?? model.files?.find((file) => file.size_bytes)?.size_bytes)} />
          <DetailItem label="Capabilities" value={capabilities} />
          <DetailItem label="Load state" value={loadState} />
        </div>

        <ModelLoadOptionsFields disabled={!!handle} options={loadOptions} onChange={onLoadOptionChange} />

        <div className="drawerActions">
          <CopyButton disabled={!modelPath} onCopy={onCopyPath} />
          {handle ? (
            <button onClick={onUnload}>
              <Square size={15} />
              Unload
            </button>
          ) : (
            <button onClick={onLoad}>
              <Play size={15} />
              Load
            </button>
          )}
          <button disabled={!modelPath} onClick={onReveal}>
            <FolderOpen size={15} />
            Reveal
          </button>
          <button className="dangerAction" disabled={!!handle} title={handle ? "Unload the model before deleting it" : undefined} onClick={onDelete}>
            <Trash2 size={15} />
            Delete
          </button>
        </div>
      </aside>
    </div>
  );
}

function DetailItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="detailItem">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function ApiEndpoint({
  title,
  value,
  disabled,
  onCopy,
}: {
  title: string;
  value: string;
  disabled?: boolean;
  onCopy: () => Promise<boolean> | boolean;
}) {
  return (
    <div className="apiEndpoint">
      <span>{title}</span>
      <code>{value}</code>
      <CopyButton disabled={disabled} onCopy={onCopy} />
    </div>
  );
}

function CopyButton({ disabled, onCopy }: { disabled?: boolean; onCopy: () => Promise<boolean> | boolean }) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    const ok = await onCopy();
    if (!ok) return;
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  }

  return (
    <button className={`copyButton ${copied ? "copied" : ""}`} disabled={disabled} onClick={handleCopy}>
      {copied ? <Check size={15} /> : <Copy size={15} />}
      {copied ? "Copied" : "Copy"}
    </button>
  );
}

function SettingsPanel({
  modelsDirectory,
  hfToken,
  onTokenChange,
  onNotice,
}: {
  modelsDirectory: string;
  hfToken: string;
  onTokenChange: (token: string) => void;
  onNotice: (message: string) => void;
}) {
  const [authMessage, setAuthMessage] = useState("Token not checked.");

  function updateToken(token: string) {
    window.localStorage.setItem("deeplocal:hf-token", token);
    onTokenChange(token);
  }

  async function checkToken(repo?: string, filename?: string) {
    const res = await fetch(`${API_BASE}/runtime/huggingface/auth-check`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ token: hfToken || null, repo, filename }),
    });
    const data = await res.json();
    const message = data.message ?? "No response from auth check.";
    setAuthMessage(message);
    onNotice(message);
  }

  return (
    <div className="pane settingsGrid">
      <label>
        Default backend
        <select defaultValue="llama.cpp">
          <option value="llama.cpp">llama.cpp</option>
        </select>
      </label>
      <label>
        API base
        <input value={API_BASE} readOnly />
      </label>
      <label>
        Model directory
        <input value={modelsDirectory} readOnly />
      </label>
      <label>
        Hugging Face token
        <input
          type="password"
          value={hfToken}
          onChange={(event) => updateToken(event.target.value)}
          placeholder="hf_... or Bearer hf_..."
        />
      </label>
      <section className="authPanel">
        <h2>Hugging Face access</h2>
        <p>{authMessage}</p>
        <div className="headerActions">
          <button onClick={() => checkToken()}>Check Token</button>
          <button onClick={() => checkToken("google/gemma-3-1b-it-qat-q4_0-gguf", "gemma-3-1b-it-q4_0.gguf")}>
            Check Gemma
          </button>
        </div>
      </section>
    </div>
  );
}

function huggingFaceHeaders(token: string) {
  return token.trim() ? { "x-huggingface-token": token.trim() } : undefined;
}

function createLocalDescriptor(id: string, path: string): ModelDescriptor & Record<string, unknown> {
  const now = new Date().toISOString();
  return {
    id,
    name: id,
    family: null,
    source: "local",
    repo: null,
    revision: null,
    format: "gguf",
    quantization: null,
    size_bytes: null,
    context_length: null,
    capabilities: ["chat", "completion"],
    files: [{ filename: path, path, size_bytes: null, sha256: null }],
    local_path: path,
    created_at: now,
    updated_at: now,
  };
}

function modelIdFromPath(path: string) {
  const filename = path.trim().split(/[\\/]/).filter(Boolean).pop() ?? "";
  const stem = filename.replace(/\.gguf$/i, "");
  const id = stem.replace(/[^a-zA-Z0-9._-]+/g, "-").replace(/^-+|-+$/g, "");
  return id || "";
}

function resolveModelPath(path: string | null | undefined, modelsDirectory: string) {
  if (!path) return null;
  if (isAbsolutePath(path)) return normalizeDisplayPath(path);

  const trimmedPath = path.replace(/^\.\//, "");
  const normalizedModelsDirectory = normalizeDisplayPath(modelsDirectory).replace(/\/+$/, "");

  if (trimmedPath === "models") return normalizedModelsDirectory;
  if (trimmedPath.startsWith("models/")) {
    return normalizeDisplayPath(`${normalizedModelsDirectory}/${trimmedPath.slice("models/".length)}`);
  }

  const projectRoot = normalizedModelsDirectory.endsWith("/models")
    ? normalizedModelsDirectory.slice(0, -"models".length).replace(/\/+$/, "")
    : normalizedModelsDirectory;
  return normalizeDisplayPath(`${projectRoot}/${trimmedPath}`);
}

function normalizeDisplayPath(path: string) {
  let normalized = path;
  while (normalized.includes("/./")) {
    normalized = normalized.replace(/\/\.\//g, "/");
  }
  return normalized;
}

function isAbsolutePath(path: string) {
  return path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path);
}

function formatBytes(bytes: number) {
  if (!bytes) return "0 GB";
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function formatFileSize(bytes?: number | null) {
  if (!bytes) return "Unknown";
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function formatTransferredSize(bytes: number) {
  if (!bytes) return "0 MB";
  return formatFileSize(bytes);
}

function downloadSpeedAndEta(job: DownloadJob) {
  const speed = formatDownloadSpeed(job.speed_bytes_per_sec);
  if (!job.total_bytes) return `${speed} / ETA unknown`;
  return `${speed} / ${formatEta(job.eta_seconds)}`;
}

function formatDownloadSpeed(bytesPerSecond?: number | null) {
  if (!bytesPerSecond || !Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) {
    return "Speed pending";
  }
  return `${(bytesPerSecond / 1024 / 1024).toFixed(1)} MB/s`;
}

function formatEta(seconds?: number | null) {
  if (seconds === null || seconds === undefined) return "ETA pending";
  if (seconds <= 0) return "ETA now";
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  if (minutes < 1) return `ETA ${remainingSeconds}s`;
  if (minutes < 60) return `ETA ${minutes}m ${remainingSeconds}s`;
  const hours = Math.floor(minutes / 60);
  return `ETA ${hours}h ${minutes % 60}m`;
}

function flattenSearchResults(results: HuggingFaceResult[]) {
  return results.flatMap((result) =>
    result.files.map((file) => ({
      repo: result.repo,
      filename: file.filename,
      size_bytes: file.size_bytes,
      downloads: result.downloads,
      likes: result.likes,
    })),
  );
}

function sortSearchFiles(files: HuggingFaceModelFile[], sortBy: SearchSort) {
  return [...files].sort((a, b) => {
    if (sortBy === "name") return a.filename.localeCompare(b.filename) || a.repo.localeCompare(b.repo);
    if (sortBy === "likes") return compareNumbersDesc(a.likes, b.likes) || a.filename.localeCompare(b.filename);
    if (sortBy === "smallest-file") return compareNumbersAsc(a.size_bytes, b.size_bytes) || a.filename.localeCompare(b.filename);
    if (sortBy === "largest-file") return compareNumbersDesc(a.size_bytes, b.size_bytes) || a.filename.localeCompare(b.filename);
    return compareNumbersDesc(a.downloads, b.downloads) || a.filename.localeCompare(b.filename);
  });
}

function modelQuantizationLabel(filename: string) {
  const match = filename.match(/(?:^|[-_.])((?:BF|F|Q|IQ)\d{1,2}(?:_[A-Z0-9]+)?|BF16|F16)(?=\.|[-_])/i);
  return match?.[1]?.toUpperCase() ?? "GGUF";
}

function compareNumbersDesc(a?: number | null, b?: number | null) {
  return compareNumbersAsc(b, a);
}

function compareNumbersAsc(a?: number | null, b?: number | null) {
  const left = typeof a === "number" ? a : Number.POSITIVE_INFINITY;
  const right = typeof b === "number" ? b : Number.POSITIVE_INFINITY;
  return left - right;
}

function downloadPercent(job: DownloadJob) {
  if (!job.total_bytes) return job.downloaded_bytes ? "receiving" : "pending";
  return `${Math.min(100, Math.round((job.downloaded_bytes / job.total_bytes) * 100))}%`;
}

function downloadKey(repo: string, filename: string) {
  return `${repo}::${filename}`;
}

function downloadActionLabel(job: DownloadJob) {
  if (job.status === "downloaded") return "Downloaded";
  if (job.status === "error" || job.status === "cancelled") return "Retry";
  if (job.status === "cancelling") return "Stopping";
  if (job.status === "starting") return "Starting";
  return "Stop";
}

function isActiveDownload(status: string) {
  return ["queued", "starting", "downloading", "cancelling"].includes(status);
}

function isDownloadHistory(status: string) {
  return ["downloaded", "cancelled", "error"].includes(status);
}

function downloadStatusLabel(status: string) {
  const labels: Record<string, string> = {
    queued: "Queued",
    starting: "Preparing",
    downloading: "Downloading",
    cancelling: "Stopping",
    cancelled: "Cancelled",
    downloaded: "Downloaded",
    error: "Needs attention",
  };
  return labels[status] ?? "In progress";
}

function normalizeMarkdown(content: string) {
  return unwrapMarkdownTableCodeFences(content)
    .replace(/\\\*/g, "*")
    .replace(/\\_/g, "_")
    .replace(/\\`/g, "`");
}

function unwrapMarkdownTableCodeFences(content: string) {
  return content.replace(/```([^\n`]*)\n([\s\S]*?)```/g, (match, language: string, code: string) => {
    const normalizedLanguage = language.trim().toLowerCase();
    if (normalizedLanguage && normalizedLanguage !== "markdown" && normalizedLanguage !== "md") {
      return match;
    }

    const table = code.trim();
    return isMarkdownTable(table) ? table : match;
  });
}

function isMarkdownTable(content: string) {
  const lines = content
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  if (lines.length < 2) return false;
  if (!lines.every((line) => line.includes("|"))) return false;

  return lines.some((line) => {
    const cells = line
      .split("|")
      .map((cell) => cell.trim())
      .filter(Boolean);

    return cells.length > 0 && cells.every((cell) => /^:?-{3,}:?$/.test(cell));
  });
}

function titleFromPrompt(prompt: string) {
  const title = prompt.replace(/\s+/g, " ").trim();
  return title.length > 44 ? `${title.slice(0, 41)}...` : title || "New conversation";
}

function conversationPreview(conversation: ChatConversation) {
  const lastMessage = conversation.messages.at(-1);
  if (!lastMessage) return "No messages yet";
  const preview = lastMessage.content.replace(/\s+/g, " ").trim();
  return preview.length > 58 ? `${preview.slice(0, 55)}...` : preview;
}

function formatConversationTime(value: string) {
  const timestamp = Date.parse(value);
  if (Number.isNaN(timestamp)) return "Saved";

  const formatter = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
  return formatter.format(timestamp);
}

async function streamChatCompletion(model: string, messages: ChatMessage[], signal: AbortSignal, onToken: (token: string) => void) {
  const res = await fetch(`${API_BASE}/v1/chat/completions`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    signal,
    body: JSON.stringify({
      model,
      stream: true,
      messages,
    }),
  });
  if (!res.ok) throw new Error(await res.text());
  if (!res.body) throw new Error("Streaming response body is not available.");

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { value, done } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const events = buffer.split("\n\n");
    buffer = events.pop() ?? "";

    for (const event of events) {
      const data = event
        .split("\n")
        .filter((line) => line.startsWith("data:"))
        .map((line) => line.slice(5).trim())
        .join("\n");

      if (!data || data === "[DONE]") continue;
      const parsed = JSON.parse(data);
      const token = parsed.choices?.[0]?.delta?.content;
      if (typeof token === "string") onToken(token);
    }
  }
}

createRoot(document.getElementById("root")!).render(<App />);
