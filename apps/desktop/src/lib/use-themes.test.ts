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

  it("expand fetches books once + caches", async () => {
    const themesPayload = [
      { slug: "mind-and-self", label: "Who am I really?", count: 2, order: 1 },
    ];
    const booksPayload = [
      { gutenberg_id: 1, title: "Republic", translators: ["Jowett"] },
      { gutenberg_id: 2, title: "Phaedo", translators: ["Jowett"] },
    ];
    (invoke as unknown as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce(themesPayload)
      .mockResolvedValueOnce(booksPayload);
    await themes.init();
    await themes.expand("mind-and-self");
    await themes.expand("mind-and-self");
    expect(themes.expandedSlug).toBeNull();
    expect(themes.booksByTheme["mind-and-self"]?.length).toBe(2);
    expect(
      (invoke as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(
        (c) => c[0] === "library_books_in_theme",
      ).length,
    ).toBe(1);
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
