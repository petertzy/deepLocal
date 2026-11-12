import { useCallback, useEffect, useState } from "react";
import { Download, RefreshCw, ShieldCheck, Trash2 } from "lucide-react";
import "./translation-models.css";

type DownloadJob = {
  id: string;
  repo: string;
  filename: string;
  status: string;
  downloaded_bytes: number;
  total_bytes?: number | null;
  error?: string | null;
};

type TranslationModel = {
  direction: "en-zh" | "zh-en";
  name: string;
  repository: string;
  directory: string;
  installed: boolean;
  directory_exists: boolean;
  size_bytes?: number | null;
  verified_manifest: boolean;
};

type TranslationCatalog = {
  models: TranslationModel[];
  runtime: { configured: boolean; bundled: boolean; provider: string };
};

type Props = {
  apiBase: string;
  downloads: DownloadJob[];
  onRefresh: () => Promise<void>;
};

export function TranslationModels({ apiBase, downloads, onRefresh }: Props) {
  const [catalog, setCatalog] = useState<TranslationCatalog | null>(null);
  const [notice, setNotice] = useState("");
  const [runtimeNotice, setRuntimeNotice] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const response = await fetch(`${apiBase}/runtime/translation/models`);
      if (!response.ok) throw new Error("Unable to read translation model status.");
      setCatalog(await response.json() as TranslationCatalog);
    } catch {
      setNotice("Unable to connect to the local translation model manager.");
    }
  }, [apiBase]);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => { void refresh(); }, 4000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    void checkRuntime();
  }, [apiBase]);

  async function action(endpoint: string, direction: string) {
    setBusy(direction);
    setNotice("");
    try {
      const response = await fetch(`${apiBase}/runtime/translation/models/${endpoint}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ direction }),
      });
      const body = await response.text();
      let result: { message?: string; job_id?: string };
      try { result = JSON.parse(body); } catch { result = {}; }
      if (!response.ok) throw new Error(result.message || body || `HTTP ${response.status}`);
      setNotice(endpoint === "install" ? "Download queued. Progress appears below." :
        endpoint === "delete" ? "Translation model deleted." :
        result.message || "Verification completed.");
      setConfirmDelete(null);
      await onRefresh();
      await refresh();
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Operation failed.");
    } finally {
      setBusy(null);
    }
  }

  async function cancel(jobId: string) {
    setBusy(jobId);
    try {
      const response = await fetch(`${apiBase}/runtime/downloads/cancel`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ job_id: jobId }),
      });
      if (!response.ok) throw new Error(await response.text());
      setNotice("Cancellation requested.");
      await onRefresh();
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Unable to cancel download.");
    } finally {
      setBusy(null);
    }
  }

  async function checkRuntime() {
    setBusy("runtime");
    try {
      const response = await fetch(`${apiBase}/runtime/translation/runtime/check`);
      if (!response.ok) throw new Error("Runtime check failed.");
      const result = await response.json() as { ready: boolean; message: string };
      setRuntimeNotice(result.message);
    } catch {
      setRuntimeNotice("Cannot reach the translation runtime.");
    } finally {
      setBusy(null);
    }
  }

  return (
    <section className="translationModels" aria-label="Translation models">
      <div className="sectionTitle">
        <h2>OPUS-MT translation models</h2>
        <span>Optional local downloads</span>
      </div>
      <p>Install only the directions you need. Converted models are stored in the deepLocal models folder, not inside the app or browser extension.</p>
      <div className="translationRuntime">
        <span>Runtime: external Python / CTranslate2 / SentencePiece</span>
        <button type="button" className="secondaryAction" onClick={() => void checkRuntime()} disabled={busy === "runtime"}>
          <RefreshCw size={15} /> Check runtime
        </button>
      </div>
      {runtimeNotice && <p role="status">{runtimeNotice}</p>}
      {!catalog && <p>Reading available translation models...</p>}
      {catalog?.models.map((model) => {
        const jobs = downloads.filter(job => job.repo === model.repository &&
          job.filename === `opus-mt-${model.direction}`);
        const active = jobs.find(job => ["queued", "starting", "downloading", "cancelling"].includes(job.status));
        const lastError = jobs.find(job => job.status === "error");
        const total = active?.total_bytes || 0;
        const percent = total ? Math.min(100, Math.round((active?.downloaded_bytes || 0) / total * 100)) : null;
        return (
          <div className="translationModel" key={model.direction}>
            <div className="translationModelHeading">
              <strong>{model.name}</strong>
              <span>{model.installed ? model.verified_manifest ? "Installed (verified on download)" : "Installed (manual files)" : model.directory_exists ? "Incomplete model directory" : "Not installed"}</span>
            </div>
            <small>{model.repository} · CTranslate2 · CC BY 4.0{model.size_bytes ? ` · ${(model.size_bytes / 1048576).toFixed(1)} MB on disk` : ""}</small>
            <code title={model.directory}>{model.directory}</code>
            {active && (
              <div className="translationProgress">
                <progress max={total || 1} value={total ? active.downloaded_bytes : undefined} aria-label={`${model.name} download progress`} />
                <span>{active.status} · {percent === null ? "Preparing download" : `${percent}%`} · {(active.downloaded_bytes / 1048576).toFixed(1)} MB</span>
              </div>
            )}
            {lastError && !active && !model.installed &&
              <p className="translationError" role="alert">{lastError.error || "Last download failed. Retry when ready."}</p>}
            <div className="translationActions">
              {!model.directory_exists && !active &&
                <button type="button" onClick={() => void action("install", model.direction)} disabled={busy !== null}>
                  <Download size={15} /> Install
                </button>}
              {active &&
                <button type="button" className="secondaryAction" onClick={() => void cancel(active.id)} disabled={busy !== null}>
                  Cancel download
                </button>}
              {model.directory_exists && <>
                {model.installed && model.verified_manifest &&
                  <button type="button" className="secondaryAction" onClick={() => void action("verify", model.direction)} disabled={busy !== null}>
                    <ShieldCheck size={15} /> Verify files
                  </button>}
                {confirmDelete === model.direction
                  ? <>
                    <span>Delete these local model files?</span>
                    <button type="button" className="dangerAction" onClick={() => void action("delete", model.direction)} disabled={busy !== null}>Confirm delete</button>
                    <button type="button" className="secondaryAction" onClick={() => setConfirmDelete(null)}>Keep</button>
                  </>
                  : <button type="button" className="dangerAction" onClick={() => setConfirmDelete(model.direction)} disabled={busy !== null}>
                      <Trash2 size={15} /> Delete
                    </button>}
              </>}
            </div>
          </div>
        );
      })}
      <p className="translationFootnote">Model files are downloaded from Hugging Face and checked before installation. The application currently requires a separately configured Python runtime; no model weights are bundled.</p>
      {notice && <p role="status">{notice}</p>}
    </section>
  );
}
