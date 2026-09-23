import { marked } from "marked";
import { sanitizeHtml } from "./sanitizeHtml";

marked.setOptions({ gfm: true, breaks: true });

export function renderMarkdown(body: string): string {
  return sanitizeHtml(marked.parse(body, { async: false }) as string);
}
