import * as monaco from "monaco-editor";
import { conf as htmlConf, language as htmlLanguage } from "monaco-editor/esm/vs/basic-languages/html/html.js";

const ERB_OPEN: typeof htmlLanguage.tokenizer.root = [
  [/<%#/, { token: "comment", next: "@erbComment" }],
  [/<%[-=]?/, { token: "delimiter", next: "@erb" }],
];

const language = {
  ...htmlLanguage,
  tokenizer: {
    ...htmlLanguage.tokenizer,
    root: [...ERB_OPEN, ...htmlLanguage.tokenizer.root],
    // Split quoted attribute values into a state so ERB inside attributes
    // (e.g. href="<%= url %>") gets tokenized instead of swallowed by
    // html's single-token ["..."] rule.
    otherTag: [
      ...ERB_OPEN,
      [/\/?>/, "delimiter", "@pop"],
      [/"/, { token: "attribute.value", next: "@attrValueDouble" }],
      [/'/, { token: "attribute.value", next: "@attrValueSingle" }],
      [/[\w\-]+/, "attribute.name"],
      [/=/, "delimiter"],
      [/[ \t\r\n]+/],
    ],
    attrValueDouble: [
      [/<%[-=]?/, { token: "delimiter", next: "@erb" }],
      [/"/, { token: "attribute.value", next: "@pop" }],
      [/[^"<]+/, "attribute.value"],
    ],
    attrValueSingle: [
      [/<%[-=]?/, { token: "delimiter", next: "@erb" }],
      [/'/, { token: "attribute.value", next: "@pop" }],
      [/[^'<]+/, "attribute.value"],
    ],
    // Simplified Ruby subset: enough for view code, no state transitions,
    // so Ruby's `end` keyword cannot pop the ERB state early.
    erb: [
      [/-?%>/, { token: "delimiter", next: "@pop" }],
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
      [/./, "identifier"],
    ],
    erbComment: [
      [/-?%>/, { token: "comment", next: "@pop" }],
      [/./, "comment"],
    ],
  },
};

monaco.languages.register({ id: "erb", extensions: [".erb"], aliases: ["ERB"] });
monaco.languages.setLanguageConfiguration("erb", htmlConf);
monaco.languages.setMonarchTokensProvider("erb", language);
