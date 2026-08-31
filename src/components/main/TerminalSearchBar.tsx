import { useEffect, useRef, useState } from "react";
import {
  X,
  ArrowUp,
  ArrowDown,
  Regex,
  CaseSensitive,
  WholeWord,
} from "lucide-react";
import type { SearchAddon } from "@xterm/addon-search";

interface TerminalSearchBarProps {
  searchAddon: SearchAddon;
  onClose: () => void;
}

interface SearchOptions {
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
}

const DECORATIONS = {
  matchBackground: "#45475a",
  matchBorder: "#f9e2af",
  matchOverviewRuler: "#f9e2af",
  activeMatchBackground: "#585b70",
  activeMatchBorder: "#f38ba8",
  activeMatchColorOverviewRuler: "#f38ba8",
};

export default function TerminalSearchBar({
  searchAddon,
  onClose,
}: TerminalSearchBarProps) {
  const [query, setQuery] = useState("");
  const [options, setOptions] = useState<SearchOptions>({
    regex: false,
    caseSensitive: false,
    wholeWord: false,
  });
  const [results, setResults] = useState<{
    index: number;
    count: number;
  } | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  useEffect(() => {
    const disposable = searchAddon.onDidChangeResults((r) => {
      setResults(
        r.resultCount > 0 ? { index: r.resultIndex, count: r.resultCount } : null,
      );
    });
    return () => disposable.dispose();
  }, [searchAddon]);

  useEffect(() => {
    if (!query) {
      searchAddon.clearDecorations();
      return;
    }
    searchAddon.findNext(query, { ...options, ...DECORATIONS, incremental: true });
  }, [query, options, searchAddon]);

  const runSearch = (direction: "next" | "prev") => {
    if (!query) return;
    if (direction === "next") {
      searchAddon.findNext(query, { ...options, ...DECORATIONS });
    } else {
      searchAddon.findPrevious(query, { ...options, ...DECORATIONS });
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Enter") {
      e.preventDefault();
      runSearch(e.shiftKey ? "prev" : "next");
    }
  };

  const toggle = (key: keyof SearchOptions) =>
    setOptions((o) => ({ ...o, [key]: !o[key] }));

  const toggleClass = (active: boolean) =>
    `rounded p-0.5 transition-colors ${
      active
        ? "bg-[var(--color-surface2)] text-[var(--color-text)]"
        : "text-[var(--color-overlay1)] hover:text-[var(--color-text)]"
    }`;

  return (
    <div className="absolute right-2 top-2 z-10 flex items-center gap-1 rounded-md border border-[var(--color-surface1)] bg-[var(--color-surface0)] px-1.5 py-1 shadow-lg">
      <input
        ref={inputRef}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Search"
        spellCheck={false}
        className="w-40 rounded bg-[var(--color-surface1)] px-2 py-0.5 text-xs text-[var(--color-text)] placeholder-[var(--color-overlay0)] outline-none focus:ring-1 focus:ring-[var(--color-blue)]"
      />
      <span className="min-w-12 text-right text-[10px] tabular-nums text-[var(--color-overlay1)]">
        {results ? `${results.index + 1}/${results.count}` : query ? "0/0" : ""}
      </span>
      <button
        onClick={() => toggle("caseSensitive")}
        className={toggleClass(options.caseSensitive)}
        title="Match case"
      >
        <CaseSensitive size={13} />
      </button>
      <button
        onClick={() => toggle("wholeWord")}
        className={toggleClass(options.wholeWord)}
        title="Match whole word"
      >
        <WholeWord size={13} />
      </button>
      <button
        onClick={() => toggle("regex")}
        className={toggleClass(options.regex)}
        title="Use regular expression"
      >
        <Regex size={13} />
      </button>
      <button
        onClick={() => runSearch("prev")}
        className={toggleClass(false)}
        title="Previous match (Shift+Enter)"
      >
        <ArrowUp size={13} />
      </button>
      <button
        onClick={() => runSearch("next")}
        className={toggleClass(false)}
        title="Next match (Enter)"
      >
        <ArrowDown size={13} />
      </button>
      <button
        onClick={onClose}
        className={toggleClass(false)}
        title="Close (Escape)"
      >
        <X size={13} />
      </button>
    </div>
  );
}
