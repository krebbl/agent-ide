const ALLOWED_TAGS = new Set([
  "P", "BR", "HR", "H1", "H2", "H3", "H4", "H5", "H6",
  "UL", "OL", "LI", "BLOCKQUOTE",
  "A", "STRONG", "B", "EM", "I", "U", "S", "STRIKE", "DEL",
  "CODE", "PRE", "TT", "SUB", "SUP",
  "TABLE", "THEAD", "TBODY", "TFOOT", "TR", "TH", "TD",
  "SPAN", "DIV",
]);

const ALLOWED_STYLE_PROPS = new Set(["color", "background-color"]);

function safeUrl(url: string): string | null {
  const trimmed = url.trim().toLowerCase();
  if (trimmed.startsWith("javascript:") || trimmed.startsWith("data:")) return null;
  return url;
}

function safeStyle(style: string): string | null {
  const parts = style
    .split(";")
    .map((p) => p.trim())
    .filter((p) => {
      const prop = p.split(":")[0]?.trim().toLowerCase();
      return prop ? ALLOWED_STYLE_PROPS.has(prop) : false;
    });
  return parts.length > 0 ? parts.join("; ") : null;
}

const DROPPED_TAGS = new Set(["SCRIPT", "STYLE", "IFRAME", "OBJECT", "EMBED", "NOSCRIPT"]);

function sanitizeNode(node: Element, out: Node, doc: Document) {
  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      out.appendChild(doc.createTextNode(child.textContent ?? ""));
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue;
    const el = child as Element;
    const tag = el.tagName;
    if (DROPPED_TAGS.has(tag)) continue;
    if (!ALLOWED_TAGS.has(tag)) {
      sanitizeNode(el, out, doc);
      continue;
    }
    const copy = doc.createElement(tag.toLowerCase());
    if (tag === "A") {
      const href = el.getAttribute("href");
      const safe = href ? safeUrl(href) : null;
      if (safe) copy.setAttribute("href", safe);
      copy.setAttribute("target", "_blank");
      copy.setAttribute("rel", "noreferrer");
    }
    const style = el.getAttribute("style");
    if (style) {
      const cleaned = safeStyle(style);
      if (cleaned) copy.setAttribute("style", cleaned);
    }
    if (tag === "TH" || tag === "TD") copy.setAttribute("colSpan", el.getAttribute("colspan") ?? "");
    sanitizeNode(el, copy, doc);
    out.appendChild(copy);
  }
}

export function sanitizeHtml(html: string): string {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  const root = doc.body;
  const clean = doc.createElement("div");
  sanitizeNode(root, clean, doc);
  return clean.innerHTML;
}
