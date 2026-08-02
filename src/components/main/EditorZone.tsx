import { useEffect, useRef } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import { FileCode } from "lucide-react";
import { useEditorStore, languageFromPath } from "../../stores/editorStore";
import { useUiStore } from "../../stores/uiStore";
import { monaco } from "../../utils/monacoSetup";
import { installLsp, contentChanged } from "../../services/lsp/coordinator";
import { registerProviders } from "../../services/lsp/providers";
import { installEditorOpener } from "../../services/navigation";
import { installPathLinkProviders } from "../../services/pathLinks";
import TabStrip from "../ui/TabStrip";

export default function EditorZone() {
  const { openFiles, activePath, setActive, closeFile, updateContent, saveActive } =
    useEditorStore();
  const activeFile = openFiles.find((f) => f.path === activePath) ?? null;
  const pendingReveal = useEditorStore((s) => s.pendingReveal);
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);

  useEffect(() => {
    if (!pendingReveal || pendingReveal.path !== activePath) return;
    const editor = editorRef.current;
    if (!editor) return;
    let attempts = 0;
    const tryReveal = () => {
      if (editor.getModel()?.uri.path !== pendingReveal.path) {
        if (attempts++ < 20) setTimeout(tryReveal, 50);
        return;
      }
      const position = {
        lineNumber: pendingReveal.line,
        column: pendingReveal.column,
      };
      editor.setPosition(position);
      editor.revealPositionInCenterIfOutsideViewport(position);
      editor.focus();
      useEditorStore.getState().setPendingReveal(null);
    };
    tryReveal();
  }, [pendingReveal, activePath]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        e.preventDefault();
        void saveActive();
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "w") {
        if (useUiStore.getState().focusedZone !== "editor") return;
        const { activePath, closeFile } = useEditorStore.getState();
        if (activePath) {
          e.preventDefault();
          closeFile(activePath);
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [saveActive]);

  const handleMount: OnMount = (editor) => {
    editorRef.current = editor;
    installLsp();
    registerProviders();
    installEditorOpener();
    installPathLinkProviders();
    editor.onDidFocusEditorText(() => {
      useUiStore.getState().setFocusedZone("editor");
    });
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      void saveActive();
    });
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyW, () => {
      const { activePath, closeFile } = useEditorStore.getState();
      if (activePath) closeFile(activePath);
    });
  };

  const fileName = (path: string) => path.split("/").pop() ?? path;

  return (
    <div
      className="flex h-full flex-col"
      onPointerDown={() => useUiStore.getState().setFocusedZone("editor")}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-[var(--color-surface0)] px-2">
        <FileCode size={14} className="text-[var(--color-blue)]" />
        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
          Editor
        </span>
        <TabStrip
          tabs={openFiles.map((file) => ({
            id: file.path,
            title: fileName(file.path),
            tooltip: file.path,
            badge: file.dirty ? (
              <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--color-peach)]" />
            ) : undefined,
          }))}
          activeId={activePath}
          onSelect={setActive}
          onClose={closeFile}
        />
      </div>
      <div className="flex flex-1 overflow-hidden">
        {activeFile ? (
          <Editor
            path={activeFile.path}
            language={languageFromPath(activeFile.path)}
            value={activeFile.content}
            theme="catppuccin-mocha"
            onMount={handleMount}
            onChange={(value, event) => {
              updateContent(activeFile.path, value ?? "");
              contentChanged(activeFile.projectId, activeFile.path, event.changes);
            }}
            options={{
              fontSize: 13,
              fontFamily: "'SF Mono', Menlo, Monaco, 'Courier New', monospace",
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              automaticLayout: true,
              tabSize: 2,
              padding: { top: 8 },
            }}
          />
        ) : (
          <div className="flex flex-1 items-center justify-center p-4">
            <span className="text-sm text-[var(--color-overlay0)]">
              No files open
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
