import { library } from "./use-library.svelte.js";

export type ComponentStatus = "ready" | "loading" | "error" | "idle";

export type StatusRow = {
  key: "paraphrase" | "judge" | "embedder" | "catalogue";
  label: string;
  detail: string;
  status: ComponentStatus;
  lastCheckedAt: number | null;
  canRetry: boolean;
  retry?: () => void | Promise<void>;
};

export type AggregateState = "green" | "amber" | "red";

class SystemStatusStore {
  lastChecked = $state<Record<StatusRow["key"], number | null>>({
    paraphrase: null,
    judge: null,
    embedder: null,
    catalogue: null,
  });

  rows = $derived.by((): StatusRow[] => {
    return [
      {
        key: "paraphrase",
        label: "Paraphrase model",
        detail: "Gemma 3 4B",
        status: "idle",
        lastCheckedAt: this.lastChecked.paraphrase,
        canRetry: false,
      },
      {
        key: "judge",
        label: "Faithfulness model",
        detail: "DeBERTa",
        status: "idle",
        lastCheckedAt: this.lastChecked.judge,
        canRetry: false,
      },
      {
        key: "embedder",
        label: "Embedder",
        detail: library.embedderError
          ? "bge-small · error"
          : library.embedderReady
            ? "bge-small"
            : `bge-small · loading ${embedderPercent()}%`,
        status: library.embedderError
          ? "error"
          : library.embedderReady
            ? "ready"
            : "loading",
        lastCheckedAt: this.lastChecked.embedder,
        canRetry: !!library.embedderError,
      },
      {
        key: "catalogue",
        label: "Library catalogue",
        detail: library.manifestError
          ? "offline"
          : library.manifestLoading
            ? "fetching"
            : `${library.manifest.length} books`,
        status: library.manifestError
          ? "error"
          : library.manifestLoading
            ? "loading"
            : "ready",
        lastCheckedAt: this.lastChecked.catalogue,
        canRetry: !!library.manifestError,
        retry: () => library.retryManifest(),
      },
    ];
  });

  aggregate = $derived.by((): AggregateState => {
    if (this.rows.some((r) => r.status === "error")) return "red";
    if (this.rows.some((r) => r.status === "loading")) return "amber";
    return "green";
  });

  init(): void {
    $effect.root(() => {
      $effect(() => {
        if (library.embedderReady) this.lastChecked.embedder = Date.now();
      });
      $effect(() => {
        if (library.manifest.length > 0 && !library.manifestError) {
          this.lastChecked.catalogue = Date.now();
        }
      });
    });
  }
}

function embedderPercent(): number {
  const p = library.downloadProgress["bge-small"];
  if (!p || !p.total) return 0;
  return Math.min(100, Math.round((p.bytes / p.total) * 100));
}

export const systemStatus = new SystemStatusStore();
