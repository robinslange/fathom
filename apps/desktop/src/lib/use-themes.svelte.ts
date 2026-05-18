import { invoke } from "@tauri-apps/api/core";

export type ThemeView = {
  slug: string;
  label: string;
  count: number;
  order: number;
};

export type ThemeBookSummary = {
  gutenberg_id: number;
  title: string;
  translators: string[];
};

class ThemesStore {
  themes = $state<ThemeView[]>([]);
  themesLoading = $state(true);
  themesError = $state<string | null>(null);

  expandedSlug = $state<string | null>(null);
  booksByTheme = $state<Record<string, ThemeBookSummary[]>>({});
  loadingTheme = $state<string | null>(null);

  reset(): void {
    this.themes = [];
    this.themesLoading = true;
    this.themesError = null;
    this.expandedSlug = null;
    this.booksByTheme = {};
    this.loadingTheme = null;
  }

  async init(): Promise<void> {
    this.themesLoading = true;
    this.themesError = null;
    try {
      this.themes = await invoke<ThemeView[]>("library_themes");
    } catch (e) {
      this.themesError = e instanceof Error ? e.message : String(e);
    } finally {
      this.themesLoading = false;
    }
  }

  async retry(): Promise<void> {
    await this.init();
  }

  async expand(slug: string): Promise<void> {
    if (this.expandedSlug === slug) {
      this.expandedSlug = null;
      return;
    }
    this.expandedSlug = slug;
    if (this.booksByTheme[slug]) return;
    this.loadingTheme = slug;
    try {
      const books = await invoke<ThemeBookSummary[]>("library_books_in_theme", { slug });
      this.booksByTheme = { ...this.booksByTheme, [slug]: books };
    } catch (e) {
      this.themesError = e instanceof Error ? e.message : String(e);
    } finally {
      this.loadingTheme = null;
    }
  }

  collapse(): void {
    this.expandedSlug = null;
  }
}

export const themes = new ThemesStore();
