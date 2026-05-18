import { vi } from "vitest";

// Polyfill Svelte 5 runes for Node test environment
global.$state = function (initial: any) {
  return initial;
} as any;

global.$effect = {
  root: (fn: () => void) => {
    fn();
  },
} as any;

global.$derived = {
  by: (fn: () => any) => {
    return fn();
  },
} as any;
