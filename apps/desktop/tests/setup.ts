import "@testing-library/jest-dom/vitest";

const storage = new Map<string, string>();
Object.defineProperty(window, "localStorage", {
  value: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, String(value)),
    removeItem: (key: string) => storage.delete(key),
    clear: () => storage.clear(),
  },
  configurable: true,
});

Object.defineProperty(window, "scrollTo", { value: () => undefined, writable: true });
Object.defineProperty(window, "requestAnimationFrame", {
  value: (callback: FrameRequestCallback) => window.setTimeout(() => callback(Date.now()), 0),
  writable: true,
});
Object.defineProperty(window, "cancelAnimationFrame", { value: (id: number) => window.clearTimeout(id), writable: true });

const tauriStubs = {
  "@tauri-apps/api/core": { invoke: async () => "http://127.0.0.1:14567" },
  "@tauri-apps/api/event": { listen: async () => () => undefined },
  "@tauri-apps/plugin-dialog": { open: async () => null },
  "@tauri-apps/plugin-opener": { openUrl: async () => undefined },
  "@tauri-apps/plugin-updater": { check: async () => null },
};

void tauriStubs;