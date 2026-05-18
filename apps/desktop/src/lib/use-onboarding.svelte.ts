import { invoke } from "@tauri-apps/api/core";
import { library } from "./use-library.svelte.js";

const MODEL_IDS = ["gemma3-4b", "deberta-nli", "deberta-nli-tokenizer", "bge-small", "bge-small-tokenizer"] as const;

class OnboardingStore {
  completed = $state(false);
  checking = $state(true);
  dismissing = $state(false);

  catalogueReady = $derived.by(() => {
    return library.manifest.length > 0 && library.manifestError === null;
  });

  modelsBytes = $derived.by(() => {
    let bytes = 0;
    let total = 0;
    for (const id of MODEL_IDS) {
      const p = library.downloadProgress[id];
      if (p) {
        bytes += p.bytes;
        total += p.total ?? 0;
      }
    }
    return { bytes, total };
  });

  modelsPercent = $derived.by(() => {
    const { bytes, total } = this.modelsBytes;
    if (!total) return 0;
    return Math.min(100, Math.round((bytes / total) * 100));
  });

  modelsReady = $derived(library.embedderReady);

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

  async complete(): Promise<void> {
    if (!this.canDismiss) return;
    this.dismissing = true;
    try {
      await invoke("onboarding_complete");
      this.completed = true;
    } finally {
      this.dismissing = false;
    }
  }
}

export const onboarding = new OnboardingStore();
export { MODEL_IDS };
