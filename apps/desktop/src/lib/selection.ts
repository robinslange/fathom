export type Para = { byteStart: number; text: string };

export type NodeLike = {
  nodeType: number;
  data?: string;
  textContent?: string | null;
  parentElement?: ElementLike | null;
};

export type ElementLike = NodeLike & {
  childNodes: readonly NodeLike[];
  dataset?: { byteStart?: string };
  paraEl?: ElementLike | null;
};

/**
 * Translate a DOM Range endpoint to a document-absolute UTF-8 byte offset.
 *
 * @param container - The Range's startContainer or endContainer
 * @param charOffset - The Range's startOffset or endOffset (UTF-16 code-unit
 *                     index for text-node containers, child index for element
 *                     containers)
 * @param paras - Ordered list of paragraphs in the document, used to
 *                synthesise byte offsets when the container is outside any
 *                paragraph
 * @param utf8 - UTF-8 byte length function
 * @param fallback - Which paragraph bound to snap to when container is outside
 *                   any paragraph: "start" → first paragraph's beginning,
 *                   "end" → last paragraph's end
 */
export function endpointToByteOffset(
  container: NodeLike,
  charOffset: number,
  paras: readonly Para[],
  utf8: (s: string) => number,
  fallback: "start" | "end",
): number {
  // Step 1: find the nearest paragraph element ancestor.
  const paraEl: ElementLike | null | undefined =
    container.nodeType === 3
      ? container.parentElement?.paraEl
      : (container as ElementLike).paraEl;

  // Step 2: no paragraph ancestor — apply fallback.
  if (!paraEl) {
    if (paras.length === 0) return 0;
    if (fallback === "start") return paras[0].byteStart;
    const last = paras[paras.length - 1];
    return last.byteStart + utf8(last.text);
  }

  const paraByteStart = Number(paraEl.dataset?.byteStart ?? "0");

  // Step 3: text node — sum preceding siblings then add prefix within this node.
  if (container.nodeType === 3) {
    let bytes = 0;
    for (const sibling of paraEl.childNodes) {
      if (sibling === container) break;
      bytes += utf8(sibling.textContent ?? "");
    }
    bytes += utf8((container.data ?? "").slice(0, charOffset));
    return paraByteStart + bytes;
  }

  // Steps 4 & 5: element container — charOffset is a child index.
  // Walk the first min(charOffset, childNodes.length) children.
  const limit = Math.min(charOffset, paraEl.childNodes.length);
  let bytes = 0;

  if (container === paraEl) {
    // Container IS the paragraph element (triple-click case).
    for (let i = 0; i < limit; i++) {
      bytes += utf8(paraEl.childNodes[i].textContent ?? "");
    }
    return paraByteStart + bytes;
  }

  // Container is some other element inside the paragraph (e.g. <em>).
  // Sum siblings before this container, then walk children within it up to charOffset.
  for (const sibling of paraEl.childNodes) {
    if (sibling === container) break;
    bytes += utf8(sibling.textContent ?? "");
  }
  const elContainer = container as ElementLike;
  const innerLimit = Math.min(charOffset, elContainer.childNodes.length);
  for (let i = 0; i < innerLimit; i++) {
    bytes += utf8(elContainer.childNodes[i].textContent ?? "");
  }
  return paraByteStart + bytes;
}
