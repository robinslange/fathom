import { describe, expect, it, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const { invoke } = await import("@tauri-apps/api/core");
const { themes } = await import("./use-themes.svelte.js");

describe("use-themes", () => {
  beforeEach(() => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockReset();
    themes.reset();
  });

  it("loads themes from library_themes", async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
      { slug: "mind-and-self", label: "Who am I really?", count: 49, order: 1 },
      { slug: "other", label: "Other", count: 12, order: 99 },
    ]);
    await themes.init();
    expect(themes.themes.length).toBe(2);
    expect(themes.themesLoading).toBe(false);
    expect(themes.themesError).toBeNull();
  });

  it("expand caches per-slug books across switches", async () => {
    const themesPayload = [
      { slug: "mind-and-self", label: "Who am I really?", count: 2, order: 1 },
      { slug: "how-to-live", label: "How should I live?", count: 1, order: 2 },
    ];
    const minds = [
      { gutenberg_id: 1, title: "Republic", translators: ["Jowett"] },
      { gutenberg_id: 2, title: "Phaedo", translators: ["Jowett"] },
    ];
    const ethics = [
      { gutenberg_id: 3, title: "Nicomachean Ethics", translators: ["Ross"] },
    ];
    (invoke as unknown as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce(themesPayload)
      .mockResolvedValueOnce(minds)
      .mockResolvedValueOnce(ethics);
    await themes.init();
    await themes.expand("mind-and-self");
    await themes.expand("how-to-live");
    await themes.expand("mind-and-self");
    expect(themes.expandedSlug).toBe("mind-and-self");
    expect(themes.booksByTheme["mind-and-self"]?.length).toBe(2);
    expect(themes.booksByTheme["how-to-live"]?.length).toBe(1);
    expect(
      (invoke as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(
        (c) => c[0] === "library_books_in_theme",
      ).length,
    ).toBe(2);
  });

  it("collapses when expanding the same slug twice (toggle)", async () => {
    (invoke as unknown as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce([{ slug: "x", label: "X", count: 1, order: 1 }])
      .mockResolvedValueOnce([{ gutenberg_id: 1, title: "A", translators: [] }]);
    await themes.init();
    await themes.expand("x");
    await themes.expand("x");
    expect(themes.expandedSlug).toBeNull();
  });
});
