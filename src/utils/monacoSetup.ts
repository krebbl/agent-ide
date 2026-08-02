import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";
import editorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import jsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import cssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import htmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import tsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    switch (label) {
      case "json":
        return new jsonWorker();
      case "css":
      case "scss":
      case "less":
        return new cssWorker();
      case "html":
      case "handlebars":
      case "razor":
        return new htmlWorker();
      case "typescript":
      case "javascript":
        return new tsWorker();
      default:
        return new editorWorker();
    }
  },
};

monaco.editor.defineTheme("catppuccin-mocha", {
  base: "vs-dark",
  inherit: true,
  rules: [
    { token: "", foreground: "cdd6f4" },
    { token: "comment", foreground: "6c7086", fontStyle: "italic" },
    { token: "keyword", foreground: "cba6f7" },
    { token: "string", foreground: "a6e3a1" },
    { token: "number", foreground: "fab387" },
    { token: "type", foreground: "f9e2af" },
    { token: "identifier", foreground: "cdd6f4" },
    { token: "delimiter", foreground: "9399b2" },
  ],
  colors: {
    "editor.background": "#1e1e2e",
    "editor.foreground": "#cdd6f4",
    "editorLineNumber.foreground": "#6c7086",
    "editorLineNumber.activeForeground": "#bac2de",
    "editorCursor.foreground": "#f5e0dc",
    "editor.selectionBackground": "#45475a",
    "editor.lineHighlightBackground": "#31324480",
    "editorIndentGuide.background1": "#313244",
    "editorIndentGuide.activeBackground1": "#45475a",
    "editorWidget.background": "#181825",
    "editorWidget.border": "#313244",
    "editorSuggestWidget.selectedBackground": "#45475a",
    "minimap.background": "#181825",
    "scrollbar.shadow": "#11111b",
    "editorScrollbarSlider.background": "#45475a80",
    "editorScrollbarSlider.hoverBackground": "#585b7080",
  },
});

monaco.typescript.typescriptDefaults.setDiagnosticsOptions({
  noSemanticValidation: true,
  noSyntaxValidation: true,
});
monaco.typescript.javascriptDefaults.setDiagnosticsOptions({
  noSemanticValidation: true,
  noSyntaxValidation: true,
});

loader.config({ monaco });

export { monaco };
