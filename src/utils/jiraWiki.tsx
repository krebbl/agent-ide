import type { ReactNode } from "react";

// Jira wiki markup → React nodes. No HTML strings, safe by construction.

function atWordStart(text: string, i: number): boolean {
  return i === 0 || /\s/.test(text[i - 1]);
}

function atWordEnd(text: string, j: number): boolean {
  return j >= text.length || /[\s.,;:!?)\]}]/.test(text[j]);
}

function parseInline(text: string, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let plain = "";
  let k = 0;

  const flush = () => {
    if (plain) {
      nodes.push(plain);
      plain = "";
    }
  };

  let i = 0;
  while (i < text.length) {
    if (text.startsWith("{color:", i)) {
      const openEnd = text.indexOf("}", i);
      const close = text.indexOf("{color}", i + 7);
      if (openEnd !== -1 && close !== -1) {
        flush();
        nodes.push(
          <span key={`${keyPrefix}s${k++}`} style={{ color: text.slice(i + 7, openEnd) }}>
            {parseInline(text.slice(openEnd + 1, close), keyPrefix)}
          </span>,
        );
        i = close + 7;
        continue;
      }
    }

    if (text[i] === "[") {
      const close = text.indexOf("]", i);
      if (close !== -1) {
        const inner = text.slice(i + 1, close);
        if (inner.startsWith("~") && !inner.includes("\n")) {
          flush();
          const name = inner.slice(1).replace(/^accountid:/, "");
          nodes.push(
            <span key={`${keyPrefix}m${k++}`} className="text-[var(--color-blue)]">
              @{name}
            </span>,
          );
          i = close + 1;
          continue;
        }
        if (!inner.includes("\n")) {
          const bar = inner.indexOf("|");
          const label = bar === -1 ? inner : inner.slice(0, bar);
          const rawUrl = bar === -1 ? inner : inner.slice(bar + 1);
          const url = /^[a-z][a-z0-9+.-]*:\/\//i.test(rawUrl) ? rawUrl : null;
          if (url) {
            flush();
            nodes.push(
              <a key={`${keyPrefix}l${k++}`} href={url} target="_blank" rel="noreferrer">
                {label}
              </a>,
            );
            i = close + 1;
            continue;
          }
        }
      }
    }

    if (text.startsWith("{{", i)) {
      const close = text.indexOf("}}", i + 2);
      if (close !== -1) {
        flush();
        nodes.push(<code key={`${keyPrefix}c${k++}`}>{text.slice(i + 2, close)}</code>);
        i = close + 2;
        continue;
      }
    }

    if (text.startsWith("??", i)) {
      const close = text.indexOf("??", i + 2);
      if (close !== -1 && close > i + 2) {
        flush();
        nodes.push(<cite key={`${keyPrefix}q${k++}`}>{text.slice(i + 2, close)}</cite>);
        i = close + 2;
        continue;
      }
    }

    const effect = findEffect(text, i);
    if (effect) {
      flush();
      const Tag = effect.tag;
      nodes.push(
        <Tag key={`${keyPrefix}e${k++}`}>{parseInline(effect.content, keyPrefix)}</Tag>,
      );
      i = effect.next;
      continue;
    }

    plain += text[i];
    i += 1;
  }
  flush();
  return nodes;
}

type EffectTag = "strong" | "em" | "s" | "u";

function findEffect(
  text: string,
  i: number,
): { tag: EffectTag; content: string; next: number } | null {
  const ch = text[i];
  const tag: EffectTag | null =
    ch === "*" ? "strong" : ch === "_" ? "em" : ch === "-" ? "s" : ch === "+" ? "u" : null;
  if (!tag || !atWordStart(text, i)) return null;
  const j = i + 1;
  if (j >= text.length || /\s/.test(text[j])) return null;
  const close = text.indexOf(ch, j);
  if (close === -1 || close === j) return null;
  if (/\s/.test(text[close - 1]) || !atWordEnd(text, close + 1)) return null;
  return { tag, content: text.slice(j, close), next: close + 1 };
}

type WikiItem = { text: ReactNode; child: ListNode | null };
type ListNode = { ordered: boolean; items: WikiItem[] };

type FlatItem = { depth: number; ordered: boolean; text: string };

function buildList(items: FlatItem[], keyPrefix: string): ReactNode {
  const root: ListNode = { ordered: false, items: [] };
  const stack: Array<{ depth: number; node: ListNode; lastItem: WikiItem | null }> = [
    { depth: -1, node: root, lastItem: null },
  ];

  items.forEach((item, idx) => {
    let top = stack[stack.length - 1];
    while (stack.length > 1 && top.depth > item.depth) {
      stack.pop();
      top = stack[stack.length - 1];
    }
    if (top.depth === item.depth && top.node.ordered !== item.ordered) {
      stack.pop();
      top = stack[stack.length - 1];
    }
    if (top.depth < item.depth) {
      const level: ListNode = { ordered: item.ordered, items: [] };
      if (top.lastItem) {
        top.lastItem.child = level;
      } else {
        top.node.items.push({ text: null, child: level });
      }
      stack.push({ depth: item.depth, node: level, lastItem: null });
      top = stack[stack.length - 1];
    }
    const wikiItem: WikiItem = { text: parseInline(item.text, `${keyPrefix}i${idx}`), child: null };
    top.node.items.push(wikiItem);
    top.lastItem = wikiItem;
  });

  const renderList = (node: ListNode, key: string): ReactNode => {
    const Tag = node.ordered ? "ol" : "ul";
    return (
      <Tag key={key}>
        {node.items.map((it, i) => (
          <li key={i}>
            {it.text}
            {it.child ? renderList(it.child, `${key}-${i}`) : null}
          </li>
        ))}
      </Tag>
    );
  };

  return renderList(root, `${keyPrefix}list`);
}

function parseListLine(line: string): FlatItem | null {
  const match = line.match(/^(\s*)([*#-]+)\s+(.+)$/);
  if (!match) return null;
  const [, indent, marker, text] = match;
  return {
    depth: Math.floor(indent.length / 2) + marker.length - 1,
    ordered: marker.includes("#"),
    text,
  };
}

function buildTable(rows: string[], keyPrefix: string): ReactNode {
  const headerRows: string[][] = [];
  const bodyRows: string[][] = [];
  for (const line of rows) {
    if (line.includes("||")) {
      headerRows.push(line.split("||").slice(1, -1).map((c) => c.trim()));
    } else {
      bodyRows.push(line.replace(/^\|/, "").replace(/\|\s*$/, "").split("|").map((c) => c.trim()));
    }
  }
  return (
    <table key={`${keyPrefix}t`}>
      {headerRows.length > 0 && (
        <thead>
          {headerRows.map((cells, r) => (
            <tr key={`h${r}`}>
              {cells.map((cell, c) => (
                <th key={c}>{parseInline(cell, `${keyPrefix}h${r}c${c}`)}</th>
              ))}
            </tr>
          ))}
        </thead>
      )}
      <tbody>
        {bodyRows.map((cells, r) => (
          <tr key={`b${r}`}>
            {cells.map((cell, c) => (
              <td key={c}>{parseInline(cell, `${keyPrefix}b${r}c${c}`)}</td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function renderJiraWiki(text: string): ReactNode {
  const lines = text.split(/\r?\n/);
  const blocks: ReactNode[] = [];
  let para: string[] = [];
  let key = 0;

  const flushPara = () => {
    if (para.length) {
      const k = key++;
      blocks.push(<p key={`p${k}`}>{parseInline(para.join("\n"), `p${k}`)}</p>);
      para = [];
    }
  };

  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    const codeOpen = line.match(/^\{(code|noformat)(?::([^}]*))?\}\s*$/);
    if (codeOpen) {
      flushPara();
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !/^\{(?:code|noformat)\}\s*$/.test(lines[i])) {
        body.push(lines[i]);
        i += 1;
      }
      i += 1;
      blocks.push(
        <pre key={`code${key++}`}>
          <code>{body.join("\n")}</code>
        </pre>,
      );
      continue;
    }

    if (line.trim() === "{quote}") {
      flushPara();
      const body: string[] = [];
      i += 1;
      while (i < lines.length && lines[i].trim() !== "{quote}") {
        body.push(lines[i]);
        i += 1;
      }
      i += 1;
      blocks.push(<blockquote key={`q${key++}`}>{renderJiraWiki(body.join("\n"))}</blockquote>);
      continue;
    }

    const heading = line.match(/^(h[1-6])\.\s+(.*)$/);
    if (heading) {
      flushPara();
      const k = key++;
      const Tag = heading[1] as "h1" | "h2" | "h3" | "h4" | "h5" | "h6";
      blocks.push(<Tag key={`h${k}`}>{parseInline(heading[2], `h${k}`)}</Tag>);
      i += 1;
      continue;
    }

    if (/^-{4,}\s*$/.test(line)) {
      flushPara();
      blocks.push(<hr key={`hr${key++}`} />);
      i += 1;
      continue;
    }

    if (line.trimStart().startsWith("|")) {
      flushPara();
      const rows: string[] = [];
      while (i < lines.length && lines[i].trimStart().startsWith("|")) {
        rows.push(lines[i].trim());
        i += 1;
      }
      blocks.push(buildTable(rows, `t${key++}`));
      continue;
    }

    if (parseListLine(line)) {
      flushPara();
      const items: FlatItem[] = [];
      while (i < lines.length) {
        const parsed = parseListLine(lines[i]);
        if (!parsed) break;
        items.push(parsed);
        i += 1;
      }
      blocks.push(buildList(items, `l${key++}`));
      continue;
    }

    if (line.trim() === "") {
      flushPara();
      i += 1;
      continue;
    }

    para.push(line);
    i += 1;
  }
  flushPara();

  return <>{blocks}</>;
}
