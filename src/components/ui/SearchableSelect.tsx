import { useState, useRef, useEffect } from "react";
import { createPortal } from "react-dom";
import { ChevronDown, X } from "lucide-react";

interface SearchableSelectOption {
  value: string;
  label: string;
  icon?: React.ReactNode;
}

interface SearchableSelectProps {
  value: string;
  options: SearchableSelectOption[];
  onChange: (value: string) => void;
  placeholder?: string;
  emptyMessage?: string;
}

export default function SearchableSelect({
  value,
  options,
  onChange,
  placeholder = "Select...",
  emptyMessage = "No matches",
}: SearchableSelectProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [dropdownStyle, setDropdownStyle] = useState<React.CSSProperties>({});
  const [activeIndex, setActiveIndex] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const optionRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const selected = options.find((o) => o.value === value);

  const filtered = options.filter((o) =>
    o.label.toLowerCase().includes(query.toLowerCase())
  );

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    const el = optionRefs.current[activeIndex];
    if (el) {
      el.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
  }, [activeIndex]);

  useEffect(() => {
    if (!isOpen) return;

    const handleClick = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setIsOpen(false);
      }
    };

    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleEscape);

    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [isOpen]);

  const openDropdown = () => {
    if (triggerRef.current) {
      const rect = triggerRef.current.getBoundingClientRect();
      setDropdownStyle({
        position: "fixed",
        top: rect.bottom + 4,
        left: rect.left,
        width: rect.width,
      });
    }
    const initialIndex = Math.max(0, filtered.findIndex((o) => o.value === value));
    setActiveIndex(initialIndex);
    setIsOpen(true);
    setTimeout(() => inputRef.current?.focus(), 0);
  };

  const handleSelect = (optionValue: string) => {
    onChange(optionValue);
    setQuery("");
    setIsOpen(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (filtered.length === 0) return;

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setActiveIndex((i) => (i + 1) % filtered.length);
        break;
      case "ArrowUp":
        e.preventDefault();
        setActiveIndex((i) => (i - 1 + filtered.length) % filtered.length);
        break;
      case "Home":
        e.preventDefault();
        setActiveIndex(0);
        break;
      case "End":
        e.preventDefault();
        setActiveIndex(filtered.length - 1);
        break;
      case "Enter":
        e.preventDefault();
        handleSelect(filtered[activeIndex].value);
        break;
    }
  };

  const dropdown = (
    <div
      style={dropdownStyle}
      className="z-[100] flex flex-col rounded-md border border-[var(--color-surface0)] bg-[var(--color-mantle)] shadow-xl"
    >
      <div className="border-b border-[var(--color-surface0)] p-2">
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Search branches..."
          className="w-full rounded bg-[var(--color-base)] px-2 py-1 text-sm text-[var(--color-text)] placeholder-[var(--color-overlay0)] focus:outline-none"
        />
      </div>
      <div className="max-h-60 overflow-y-auto py-1">
        {filtered.length === 0 ? (
          <div className="px-3 py-2 text-xs text-[var(--color-overlay0)]">{emptyMessage}</div>
        ) : (
          filtered.map((o, idx) => {
            const isSelected = o.value === value;
            const isActive = idx === activeIndex;
            return (
              <button
                key={o.value}
                ref={(el) => { optionRefs.current[idx] = el; }}
                type="button"
                onClick={() => handleSelect(o.value)}
                onMouseEnter={() => setActiveIndex(idx)}
                className={`flex w-full items-center gap-1.5 px-3 py-1.5 text-left text-sm transition-colors ${
                  isSelected
                    ? "bg-[var(--color-surface0)] text-[var(--color-text)] hover:bg-[var(--color-surface0)]"
                    : isActive
                      ? "bg-[var(--color-surface1)] text-[var(--color-text)]"
                      : "text-[var(--color-subtext0)] hover:bg-[var(--color-surface1)] hover:text-[var(--color-text)]"
                }`}
              >
                {o.icon}
                <span className="truncate">{o.label}</span>
              </button>
            );
          })
        )}
      </div>
    </div>
  );

  return (
    <div ref={containerRef} className="relative">
      <div
        ref={triggerRef}
        onClick={openDropdown}
        className="flex w-full cursor-pointer items-center gap-2 rounded-md border border-[var(--color-surface0)] bg-[var(--color-base)] px-3 py-2 text-sm text-[var(--color-text)] hover:border-[var(--color-overlay0)]"
      >
        <span className="flex-1 truncate">
          {selected ? (
            <span className="flex items-center gap-1.5">
              {selected.icon}
              {selected.label}
            </span>
          ) : (
            <span className="text-[var(--color-overlay0)]">{placeholder}</span>
          )}
        </span>
        <div className="flex shrink-0 items-center gap-1">
          {selected && (
            <div
              onClick={(e) => {
                e.stopPropagation();
                onChange("");
                setQuery("");
              }}
              className="rounded p-0.5 text-[var(--color-overlay0)] hover:bg-[var(--color-surface0)] hover:text-[var(--color-text)]"
            >
              <X size={12} />
            </div>
          )}
          <ChevronDown
            size={14}
            className={`text-[var(--color-overlay0)] transition-transform ${isOpen ? "rotate-180" : ""}`}
          />
        </div>
      </div>

      {isOpen && createPortal(dropdown, document.body)}
    </div>
  );
}
