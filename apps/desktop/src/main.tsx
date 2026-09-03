import React, { useCallback, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import ReactMarkdown from "react-markdown";
import {
  Activity,
  ArrowRight,
  Boxes,
  Check,
  Copy,
  Cpu,
  Download,
  FolderOpen,
  Info,
  MessageSquare,
  Play,
  RefreshCw,
  Search,
  Server,
  Settings,
  Square,
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

const API_BASE = "http://127.0.0.1:14567";

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
  const [messages, setMessages] = useState<Array<{ role: "user" | "assistant"; content: string }>>([]);
  const activeModel = loaded.find((model) => model.backend !== "mock")?.id ?? loaded[0]?.id;

  async function send() {
    const prompt = input.trim();
    if (!prompt) return;
    if (!activeModel) {
      setMessages((items) => [...items, { role: "assistant", content: "No model is loaded. Load a downloaded GGUF model first." }]);
      onNotice("Load a model before chatting.");
      return;
    }
    setMessages((items) => [...items, { role: "user", content: prompt }]);
    setInput("");
    try {
      const res = await fetch(`${API_BASE}/v1/chat/completions`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          model: activeModel,
          stream: false,
          messages: [{ role: "user", content: prompt }],
        }),
      });
      const data = await res.json();
      const content = data.choices?.[0]?.message?.content ?? data.error ?? "No response";
      setMessages((items) => [...items, { role: "assistant", content }]);
      onNotice(`Chat completed with ${activeModel}.`);
    } catch {
      setMessages((items) => [...items, { role: "assistant", content: "Runtime is offline or the model is not loaded." }]);
      onNotice("Chat request failed. Start the deepLocal API and load a model.");
    }
  }

  return (
    <div className="pane chat">
      <div className="chatMeta">
        <Boxes size={18} />
        <span>Active model: {activeModel ?? "No model loaded"}</span>
      </div>
      <div className="transcript">
        {!messages.length && !activeModel ? (
          <EmptyState
            icon={<MessageSquare size={24} />}
            title="Load a model to start chatting"
            description="Choose a downloaded GGUF model first, then send prompts here through the local API."
            actionLabel="Open models"
            onAction={onOpenModels}
          />
        ) : (
          messages.map((message, index) => (
            <div className={`message ${message.role}`} key={`${message.role}-${index}`}>
              {message.role === "assistant" ? <ReactMarkdown>{normalizeMarkdown(message.content)}</ReactMarkdown> : message.content}
            </div>
          ))
        )}
      </div>
      <div className="composer">
        <input value={input} onChange={(event) => setInput(event.target.value)} onKeyDown={(event) => event.key === "Enter" && send()} />
        <button disabled={!activeModel} onClick={send}>Send</button>
      </div>
    </div>
  );
}

function Models({
  models,
  loaded,
  downloads,
  modelsDirectory,
  hfToken,
  onNotice,
  onRefresh,
}: {
  models: ModelDescriptor[];
  loaded: LoadedModel[];
  downloads: DownloadJob[];
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

  const downloadByFile = useMemo(() => {
    const items = new Map<string, DownloadJob>();
    for (const job of downloads) {
      items.set(downloadKey(job.repo, job.filename), job);
    }
    return items;
  }, [downloads]);

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
    onNotice(res.ok ? `Registered ${id}.` : `Failed to register ${id}.`);
    await onRefresh();
  }

  async function load(modelId: string) {
    const res = await fetch(`${API_BASE}/runtime/models/load`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ model_id: modelId, backend: "llama.cpp", context_length: 4096, gpu_layers: -1 }),
    });
    onNotice(res.ok ? `Loaded ${modelId}.` : await res.text());
    await onRefresh();
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
              actionLabel={searching ? undefined : "Search now"}
              onAction={searching ? undefined : searchHuggingFace}
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
                <button
                  disabled={job?.status === "cancelling"}
                  onClick={() => (canCancel ? cancelDownload(job) : downloadFile(file.repo, file.filename, file.size_bytes))}
                >
                  <Download size={15} />
                  {job ? downloadActionLabel(job) : "Download"}
                </button>
            </article>
            );
          })}
        </div>
      </section>
      <div className="modelTools">
        <input value={id} onChange={(event) => setId(event.target.value)} />
        <input value={path} onChange={(event) => setPath(event.target.value)} />
        <button onClick={register}>Register</button>
      </div>
      <div className="grid">
        {models.map((model) => {
          const handle = loaded.find((item) => item.id === model.id);
          const modelPath = resolveModelPath(model.local_path, modelsDirectory);
          return (
            <article key={model.id}>
              <h2>{model.name}</h2>
              <p>
                {model.source} / {model.format}
              </p>
              <div className="modelPath">
                <span>{modelPath ?? "No local path registered"}</span>
                <CopyButton disabled={!modelPath} onCopy={() => copyModelPath(model, modelPath)} />
              </div>
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
          onClose={() => setDetailsModelId(null)}
          onCopyPath={() => copyModelPath(detailsModel, detailsPath)}
          onLoad={() => load(detailsModel.id)}
          onUnload={() => unload(detailsModel.id)}
          onReveal={() => revealModel(detailsModel, detailsPath)}
        />
      )}
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
  onClose,
  onCopyPath,
  onLoad,
  onUnload,
  onReveal,
}: {
  model: ModelDescriptor;
  modelPath: string | null;
  handle: LoadedModel | null;
  onClose: () => void;
  onCopyPath: () => Promise<boolean>;
  onLoad: () => Promise<void>;
  onUnload: () => Promise<void>;
  onReveal: () => Promise<void>;
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
  return content
    .replace(/\\\*/g, "*")
    .replace(/\\_/g, "_")
    .replace(/\\`/g, "`");
}

createRoot(document.getElementById("root")!).render(<App />);
