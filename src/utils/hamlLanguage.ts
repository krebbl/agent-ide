import * as monaco from "monaco-editor";

// Haml Monarch grammar. Indentation is significant in Haml; Monarch cannot
// track indentation directly, so filter bodies (:markdown etc.) use a
// @rematch rule that pops back to root on the first non-indented line.
// includeLF makes "\n" part of the line so states can pop at end of line.
// "start" must point at root: Monarch defaults to the FIRST tokenizer state.
const language: monaco.languages.IMonarchLanguage = {
  includeLF: true,
  start: "root",
  tokenizer: {
    root: [
      [/^[ \t]*-#[^\n]*/, "comment"],
      [/^[ \t]*\/[^\n]*/, "comment"],
      [/^[ \t]*!!![^\n]*/, "tag"],
      [/^[ \t]*%[\w:-]+/, "tag", "@afterTag"],
      [/^[ \t]*[.#][\w-]+/, "tag", "@afterTag"],
      [/^[ \t]*=/, "keyword", "@rubyLine"],
      [/^[ \t]*[&!]=/, "", "@plainText"],
      [/^[ \t]*-/, "keyword", "@rubyLine"],
      [/^[ \t]*:[\w-]+/, "keyword", "@filterBody"],
      [/^[ \t]+/, ""],
      [/[^\n]/, "", "@plainText"],
      [/\n/, ""],
    ],
    // Transition-free Ruby subset, also usable at end of line, inside {}
    // attribute hashes and inside #{} interpolation. Excludes "\n" and "}"
    // so enclosing states keep their pop rules.
    rubyBody: [
      [/"(?:[^"\\]|\\.)*"/, "string"],
      [/'(?:[^'\\]|\\.)*'/, "string"],
      [/:[\w!?]+/, "string"],
      [/@\w+|\$\w+/, "type"],
      [
        /\b(?:and|begin|case|class|def|do|each|else|elsif|end|ensure|false|if|in|module|nil|not|or|rescue|return|then|true|unless|until|when|while|yield)\b/,
        "keyword",
      ],
      [/\d+(?:\.\d+)?/, "number"],
      [/#[^\n]*/, "comment"],
      [/[^\n}]/, "identifier"],
    ],
    afterTag: [
      [/\.[\w-]+/, "attribute.name"],
      [/#[\w-]+/, "attribute.name"],
      [/\(/, "delimiter", "@htmlAttrs"],
      [/\{/, "delimiter", "@rubyAttrs"],
      [/\//, "delimiter"],
      [/=/, "keyword", "@rubyLine"],
      [/[ \t]+/, "", "@afterTagWs"],
      [/\n/, { token: "", next: "@popall" }],
    ],
    afterTagWs: [
      [/=/, "keyword", "@rubyLine"],
      [/[^\n]/, "", "@plainText"],
      [/\n/, { token: "", next: "@popall" }],
    ],
    htmlAttrs: [
      [/\)/, { token: "delimiter", next: "@pop" }],
      [/[\w-]+/, "attribute.name"],
      [/=/, "delimiter", "@attrValue"],
      [/[ \t]+/, ""],
      [/\n/, { token: "", next: "@popall" }],
      [/./, "attribute.name"],
    ],
    attrValue: [
      [/"(?:[^"\\]|\\.)*"/, "attribute.value", "@pop"],
      [/'(?:[^'\\]|\\.)*'/, "attribute.value", "@pop"],
      [/[^\)"' \t\n]+/, "attribute.value", "@pop"],
    ],
    rubyAttrs: [
      [/\}/, { token: "delimiter", next: "@pop" }],
      { include: "rubyBody" },
      [/\n/, { token: "", next: "@popall" }],
    ],
    rubyLine: [
      { include: "rubyBody" },
      [/\n/, { token: "", next: "@popall" }],
    ],
    interp: [
      { include: "rubyBody" },
      [/\}/, { token: "delimiter", next: "@pop" }],
      [/\n/, { token: "", next: "@popall" }],
    ],
    plainText: [
      [/#\{/, "delimiter", "@interp"],
      [/[^\n#]+/, "identifier"],
      [/#/, "identifier"],
      [/\n/, { token: "", next: "@popall" }],
    ],
    filterBody: [
      // Dedented line ends the filter body; hand it back to root.
      [/^\S/, { token: "@rematch", next: "@pop" }],
      [/^[ \t]+[^\n]*/, "string"],
      [/^[ \t]*\n/, ""],
    ],
  },
};

monaco.languages.register({
  id: "haml",
  extensions: [".haml"],
  aliases: ["Haml"],
  mimetypes: ["text/x-haml"],
});
monaco.languages.setLanguageConfiguration("haml", {
  comments: { lineComment: "-#" },
  brackets: [
    ["(", ")"],
    ["[", "]"],
    ["{", "}"],
  ],
  autoClosingPairs: [
    { open: "(", close: ")" },
    { open: "[", close: "]" },
    { open: "{", close: "}" },
    { open: '"', close: '"' },
    { open: "'", close: "'" },
  ],
  surroundingPairs: [
    { open: "(", close: ")" },
    { open: "[", close: "]" },
    { open: "{", close: "}" },
    { open: '"', close: '"' },
    { open: "'", close: "'" },
  ],
});
monaco.languages.setMonarchTokensProvider("haml", language);
