<script lang="ts">
  import { systemStatus, type StatusRow } from "./use-system-status.svelte.js";

  let open = $state(false);

  function toggle() {
    open = !open;
  }

  function closeOnOutside(e: MouseEvent) {
    if (!open) return;
    const target = e.target as Node;
    const pop = document.getElementById("status-popover");
    const dot = document.getElementById("status-dot-button");
    if (pop && !pop.contains(target) && dot && !dot.contains(target)) {
      open = false;
    }
  }

  $effect(() => {
    if (open) {
      document.addEventListener("mousedown", closeOnOutside);
      return () => document.removeEventListener("mousedown", closeOnOutside);
    }
  });

  function relativeAge(ts: number | null): string {
    if (ts === null) return "—";
    const m = Math.floor((Date.now() - ts) / 60_000);
    if (m < 1) return "just now";
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    return `${h}h ago`;
  }

  function dotClass(status: StatusRow["status"]) {
    return `dot ${status}`;
  }
</script>

<button
  id="status-dot-button"
  class="status-dot {systemStatus.aggregate}"
  aria-label="System status: {systemStatus.aggregate}"
  aria-expanded={open}
  type="button"
  onclick={toggle}
></button>

{#if open}
  <div id="status-popover" class="popover" role="dialog" aria-label="System status">
    <div class="header">System</div>
    {#each systemStatus.rows as row (row.key)}
      <div class="row">
        <div class="row-head">
          <span class={dotClass(row.status)} aria-hidden="true"></span>
          <span class="row-label">{row.label}</span>
          {#if row.canRetry && row.retry}
            <button class="retry" type="button" onclick={() => row.retry?.()}>Retry</button>
          {/if}
        </div>
        <div class="row-detail">
          <span class="detail">{row.detail}</span>
          <span class="age">· checked {relativeAge(row.lastCheckedAt)}</span>
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .status-dot {
    width: 10px; height: 10px;
    border-radius: 50%;
    border: 0; padding: 0;
    cursor: pointer;
    transition: transform 100ms;
  }
  .status-dot:hover { transform: scale(1.15); }
  .status-dot:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .status-dot.green { background: var(--ok-ink, #6a8f5e); }
  .status-dot.amber { background: var(--accent, #b07a3a); }
  .status-dot.red { background: var(--error-ink, #b8484e); }

  .popover {
    position: absolute;
    top: 3rem;
    left: 6.5rem;
    width: 300px;
    background: var(--panel, #fff);
    border: 1px solid var(--rule-strong);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.12);
    padding: 0.75rem 0.9rem 0.6rem;
    z-index: 50;
  }
  .header {
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    opacity: 0.55;
    margin-bottom: 0.5rem;
  }
  .row {
    padding: 0.4rem 0;
    border-bottom: 1px solid var(--rule-faint);
  }
  .row:last-child { border-bottom: 0; }
  .row-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .row-label {
    font-size: 0.88rem;
    flex: 1;
  }
  .dot { width: 7px; height: 7px; border-radius: 50%; display: inline-block; }
  .dot.ready { background: var(--ok-ink, #6a8f5e); }
  .dot.loading { background: var(--accent, #b07a3a); }
  .dot.error { background: var(--error-ink, #b8484e); }
  .dot.idle { background: var(--rule-strong); }
  .row-detail {
    margin-top: 0.15rem;
    padding-left: 1rem;
    font-family: "IBM Plex Mono", monospace;
    font-size: 0.72rem;
    opacity: 0.55;
  }
  .age { margin-left: 0.25em; }
  .retry {
    font-family: "IBM Plex Sans", sans-serif;
    font-size: 0.72rem;
    background: transparent;
    border: 1px solid var(--rule-strong);
    padding: 0.15rem 0.5rem;
    border-radius: 3px;
    cursor: pointer;
  }
  .retry:hover { background: var(--accent-wash-hover); }
  .retry:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
</style>
