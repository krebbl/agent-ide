import { useMemo } from "react";
import { ChevronRight, ChevronDown, File } from "lucide-react";
import { useFileTreeStore, type DirEntry } from "../../stores/fileTreeStore";

interface FileTreeNodeProps {
  entry: DirEntry;
  dirPath: string;
  depth: number;
  onContextMenu: (e: React.MouseEvent, entryPath: string, isDir: boolean, name: string) => void;
}

export default function FileTreeNode({ entry, dirPath, depth, onContextMenu }: FileTreeNodeProps) {
  const { toggleDir, nodeState } = useFileTreeStore();
  const fullPath = `${dirPath}/${entry.name}`;
  const node = nodeState[fullPath];
  const isExpanded = node?.expanded ?? false;
  const isLoading = node?.loading ?? false;
  const children = node?.children ?? [];

  const iconColor = useMemo(() => {
    if (!entry.isDir) {
      const ext = entry.name.split(".").pop()?.toLowerCase() ?? "";
      switch (ext) {
        case "ts": case "tsx": return "var(--color-blue)";
        case "js": case "jsx": return "var(--color-yellow)";
        case "rs": return "var(--color-peach)";
        case "json": case "toml": case "yaml": case "yml": return "var(--color-green)";
        case "css": case "scss": return "var(--color-mauve)";
        case "md": return "var(--color-subtext0)";
        case "html": return "var(--color-red)";
        case "sh": case "bash": return "var(--color-green)";
        case "lock": return "var(--color-overlay1)";
        default: return "var(--color-overlay1)";
      }
    }
    return "var(--color-blue)";
  }, [entry.name, entry.isDir]);

  const handleClick = () => {
    if (entry.isDir) {
      toggleDir(fullPath);
    }
  };

  return (
    <>
      <div
        className="group flex cursor-pointer items-center gap-1 py-0.5 pr-2 text-sm hover:bg-[var(--color-surface0)] text-[var(--color-text)]"
        style={{ paddingLeft: `${depth * 12 + 4}px` }}
        onClick={handleClick}
        onContextMenu={(e) => onContextMenu(e, fullPath, entry.isDir, entry.name)}
      >
        {/* Chevron / spacer (14px) */}
        {entry.isDir ? (
          isLoading ? (
            <span className="flex-shrink-0 w-[14px]" />
          ) : (
            <span className="flex-shrink-0 w-[14px] text-[var(--color-blue)]">
              {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            </span>
          )
        ) : (
          <span className="flex-shrink-0 w-[14px]" />
        )}

        {/* File/folder icon */}
        {entry.isDir ? (
          <span className="flex-shrink-0" style={{ color: iconColor }}>
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              {isExpanded ? (
                <path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" />
              ) : (
                <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" />
              )}
            </svg>
          </span>
        ) : (
          <span className="flex-shrink-0" style={{ color: iconColor }}>
            <File size={14} />
          </span>
        )}
        <span className="truncate select-none">{entry.name}</span>
      </div>

      {entry.isDir && isExpanded && children.map((child) => (
        <FileTreeNode
          key={child.name}
          entry={child}
          dirPath={fullPath}
          depth={depth + 1}
          onContextMenu={onContextMenu}
        />
      ))}
    </>
  );
}
