import { invoke } from "@tauri-apps/api/core";
import { library } from "./use-library.svelte.js";

/// Three visible bars on the onboarding modal. Each groups one user-facing
/// model with its optional tokenizer, because tokenizers are tiny (~700KB)
/// and conceptually part of the same component.
type Component = {
  key: "gemma" | "deberta" | "bge";
  label: string;
  detail: string;
  modelIds: string[];
  readySignal: () => boolean;
};

const COMPONENTS: Component[] = [
  {
    key: "gemma",
    label: "Paraphrase model",
    detail: "Gemma 3 4B",
    modelIds: ["gemma3-4b"],
    readySignal: () => library.llamaReady,
  },
  {
    key: "deberta",
    label: "Faithfulness model",
    detail: "DeBERTa NLI",
    modelIds: ["deberta-nli", "deberta-nli-tokenizer"],
    readySignal: () => library.judgeReady,
  },
  {
    key: "bge",
    label: "Embedding model",
    detail: "bge-small",
    modelIds: ["bge-small", "bge-small-tokenizer"],
    readySignal: () => library.embedderReady,
  },
];

export type ComponentProgress = {
  key: Component["key"];
  label: string;
  detail: string;
  bytes: number;
  total: number | null;
  percent: number;
  bytesPerSecond: number;
  ready: boolean;
};

/// EMA smoothing factor for download speed. 0.3 = responsive but stable
/// across the 200ms throttled progress callbacks; lower = smoother but
/// laggier on speed changes.
const SPEED_EMA_ALPHA = 0.3;

type SpeedSample = {
  bytes: number;
  at: number;
  ema: number;
};

class OnboardingStore {
  completed = $state(false);
  checking = $state(true);
  dismissing = $state(false);

  // Per-component speed state. Not reactive — purely derived bookkeeping.
  // We mutate inside the components $derived.by below, which is allowed
  // because the mutation only happens when downloadProgress changes.
  private speedState = new Map<Component["key"], SpeedSample>();

  catalogueReady = $derived.by(() => {
    return library.manifest.length > 0 && library.manifestError === null;
  });

  components = $derived.by((): ComponentProgress[] => {
    const now = Date.now();
    return COMPONENTS.map((c) => {
      let bytes = 0;
      let total = 0;
      let totalKnown = false;
      for (const id of c.modelIds) {
        const p = library.downloadProgress[id];
        if (p) {
          bytes += p.bytes;
          if (p.total != null) {
            total += p.total;
            totalKnown = true;
          }
        }
      }
      const ready = c.readySignal();
      const prev = this.speedState.get(c.key);
      let bytesPerSecond = 0;
      if (prev && now > prev.at && bytes > prev.bytes) {
        const instant = ((bytes - prev.bytes) / (now - prev.at)) * 1000;
        bytesPerSecond = prev.ema * (1 - SPEED_EMA_ALPHA) + instant * SPEED_EMA_ALPHA;
      } else if (prev) {
        bytesPerSecond = prev.ema * (1 - SPEED_EMA_ALPHA);
      }
      this.speedState.set(c.key, { bytes, at: now, ema: bytesPerSecond });

      const knownTotal = totalKnown ? total : null;
      const percent = ready
        ? 100
        : knownTotal && knownTotal > 0
          ? Math.min(100, Math.round((bytes / knownTotal) * 100))
          : 0;
      return {
        key: c.key,
        label: c.label,
        detail: c.detail,
        bytes,
        total: knownTotal,
        percent,
        bytesPerSecond: ready ? 0 : bytesPerSecond,
        ready,
      };
    });
  });

  modelsReady = $derived(
    library.embedderReady && library.judgeReady && library.llamaReady,
  );

  shouldShow = $derived.by(() => {
    if (this.checking) return false;
    if (this.completed) return false;
    if (this.catalogueReady && this.modelsReady) return false;
    return true;
  });

  canDismiss = $derived(this.catalogueReady && this.modelsReady);

  async init(): Promise<void> {
    try {
      const status = await invoke<{ completed: boolean }>("onboarding_status");
      this.completed = status.completed;
    } catch {
      this.completed = false;
    } finally {
      this.checking = false;
    }
  }

  completeError = $state<string | null>(null);

  async complete(): Promise<void> {
    if (!this.canDismiss) return;
    this.dismissing = true;
    this.completeError = null;
    try {
      await invoke("onboarding_complete");
      this.completed = true;
    } catch (e) {
      this.completeError = e instanceof Error ? e.message : String(e);
    } finally {
      this.dismissing = false;
    }
  }
}

export const onboarding = new OnboardingStore();
