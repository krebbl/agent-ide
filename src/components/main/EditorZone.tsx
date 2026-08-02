import { useEffect } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import { FileCode, X } from "lucide-react";
import { useEditorStore, languageFromPath } from "../../stores/editorStore";
import { monaco } from "../../utils/monacoSetup";

export default function EditorZone() {
  const { openFiles, activePath, setActive, closeFile, updateContent, saveActive } =
    useEditorStore();
  const activeFile = openFiles.find((f) => f.path === activePath) ?? null;

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        e.preventDefault();
        void saveActive();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [saveActive]);

  const handleMount: OnMount = (editor) => {
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      void saveActive();
    });
  };

  const fileName = (path: string) => path.split("/").pop() ?? path;

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-[var(--color-surface0)] px-3">
        <FileCode size={14} className="text-[var(--color-blue)]" />
        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--color-subtext1)]">
          Editor
        </span>
      </div>
      {openFiles.length > 0 && (
        <div className="no-scrollbar flex h-9 shrink-0 items-stretch overflow-x-auto border-b border-[var(--color-surface0)] bg-[var(--color-mantle)]">
          {openFiles.map((file) => {
            const isActive = file.path === activePath;
            return (
              <div
                key={file.path}
                className={`group flex cursor-pointer items-center gap-1.5 border-r border-[var(--color-surface0)] px-3 text-xs ${
                  isActive
                    ? "bg-[var(--color-base)] text-[var(--color-text)]"
                    : "text-[var(--color-overlay1)] hover:bg-[var(--color-surface0)]"
                }`}
                onClick={() => setActive(file.path)}
                title={file.path}
              >
                <span className="select-none whitespace-nowrap">
                  {fileName(file.path)}
                </span>
                {file.dirty && (
                  <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--color-peach)]" />
                )}
                <button
                  className="ml-1 shrink-0 rounded p-0.5 opacity-0 transition-opacity hover:bg-[var(--color-surface1)] group-hover:opacity-100"
                  onClick={(e) => {
                    e.stopPropagation();
                    closeFile(file.path);
                  }}
                >
                  <X size={12} />
                </button>
              </div>
            );
          })}
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        {activeFile ? (
          <Editor
            path={activeFile.path}
            language={languageFromPath(activeFile.path)}
            value={activeFile.content}
            theme="catppuccin-mocha"
            onMount={handleMount}
            onChange={(value) => updateContent(activeFile.path, value ?? "")}
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
