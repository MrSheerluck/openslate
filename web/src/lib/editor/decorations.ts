import {
  ViewPlugin,
  ViewUpdate,
  Decoration,
  WidgetType,
  type DecorationSet,
  type EditorView,
} from "@codemirror/view";
import { syntaxTree } from "@codemirror/language";
import type { Text } from "@codemirror/state";
import type { Range } from "@codemirror/state";

// ── Line classes applied to whole lines ──

const LINE_CLASS_BY_BLOCK: Record<string, string> = {
  ATXHeading1: "cm-lp-h1",
  ATXHeading2: "cm-lp-h2",
  ATXHeading3: "cm-lp-h3",
  ATXHeading4: "cm-lp-h4",
  ATXHeading5: "cm-lp-h5",
  ATXHeading6: "cm-lp-h6",
  SetextHeading1: "cm-lp-h1",
  SetextHeading2: "cm-lp-h2",
  Blockquote: "cm-lp-blockquote-line",
  FencedCode: "cm-lp-codeblock-line",
};

// ── Syntax tokens hidden on inactive lines ──

const HIDEABLE_SYNTAX = new Set([
  "HeaderMark",
  "EmphasisMark",
  "StrongEmphasisMark",
  "CodeMark",
  "CodeInfo",
  "LinkMark",
  "LinkTitle",
  "StrikethroughMark",
  "QuoteMark",
  "ImageMark",
]);

// ── Inline mark classes (unconditional) ──

const INLINE_MARK_CLASS: Record<string, string> = {
  StrongEmphasis: "cm-lp-bold",
  Emphasis: "cm-lp-italic",
  InlineCode: "cm-lp-inline-code",
  Strikethrough: "cm-lp-strikethrough",
  Link: "cm-lp-link",
};

// ── Widgets ──

class BulletWidget extends WidgetType {
  eq(): boolean {
    return true;
  }
  toDOM(): HTMLElement {
    const span = document.createElement("span");
    span.className = "cm-lp-list-marker cm-lp-bullet";
    span.textContent = "•";
    return span;
  }
  ignoreEvent(): boolean {
    return false;
  }
}

const BULLET_WIDGET = new BulletWidget();

// ── Image widget ──

class ImageWidget extends WidgetType {
  constructor(readonly url: string, readonly alt: string) {
    super();
  }

  eq(other: ImageWidget): boolean {
    return this.url === other.url && this.alt === other.alt;
  }

  toDOM(): HTMLElement {
    const wrapper = document.createElement("span");
    wrapper.className = "cm-lp-image-wrapper";
    const img = document.createElement("img");
    img.src = this.url;
    img.alt = this.alt;
    img.className = "cm-lp-image";
    img.style.maxWidth = "100%";
    img.style.maxHeight = "400px";
    img.style.borderRadius = "6px";
    img.style.display = "block";
    img.style.margin = "4px 0";
    wrapper.appendChild(img);
    return wrapper;
  }

  ignoreEvent(): boolean {
    return false;
  }
}

// ── Table widget ──

class TableWidget extends WidgetType {
  constructor(readonly html: string) {
    super();
  }

  eq(other: TableWidget): boolean {
    return this.html === other.html;
  }

  toDOM(): HTMLElement {
    const wrapper = document.createElement("div");
    wrapper.className = "cm-lp-table-wrapper";
    wrapper.innerHTML = this.html;
    return wrapper;
  }

  ignoreEvent(): boolean {
    return false;
  }
}


class TaskCheckboxWidget extends WidgetType {
  constructor(readonly checked: boolean) {
    super();
  }

  eq(other: TaskCheckboxWidget): boolean {
    return other.checked === this.checked;
  }

  toDOM(view: EditorView): HTMLElement {
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = this.checked;
    input.className = "cm-lp-list-marker cm-lp-task-checkbox";
    input.setAttribute("contenteditable", "false");
    input.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    input.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      const pos = view.posAtDOM(input);
      if (pos < 0) return;
      const current = view.state.doc.sliceString(pos, pos + 3);
      const next = /\[x\]/i.test(current) ? "[ ]" : "[x]";
      if (current === next) return;
      view.dispatch({
        changes: { from: pos, to: pos + 3, insert: next },
      });
    });
    return input;
  }

  ignoreEvent(event: Event): boolean {
    return event.type === "mousedown" || event.type === "click";
  }
}

// ── Helpers ──

function pushReplace(
  ranges: Range<Decoration>[],
  doc: Text,
  from: number,
  to: number,
  widget?: Parameters<typeof Decoration.replace>[0],
): void {
  if (from >= to) return;
  const startLine = doc.lineAt(from);
  if (to <= startLine.to) {
    ranges.push(Decoration.replace(widget ?? {}).range(from, to));
    return;
  }
  let cursor = from;
  let first = true;
  while (cursor < to) {
    const line = doc.lineAt(cursor);
    const segEnd = Math.min(to, line.to);
    if (segEnd > cursor) {
      ranges.push(
        Decoration.replace(first ? (widget ?? {}) : {}).range(cursor, segEnd),
      );
      first = false;
    }
    cursor = line.to + 1;
  }
}

// ── Table HTML builder ──

function buildTableHTML(raw: string): string {
  const lines = raw.trim().split("\n").filter((l) => l.trim());
  if (lines.length < 2) return "";

  // Skip separator lines (|---|---|)
  const rows = lines
    .filter((l) => !/^\s*\|?\s*[-:| ]+\s*\|?\s*$/.test(l))
    .map((l) =>
      l
        .trim()
        .replace(/^\||\|$/g, "") // Remove leading/trailing pipes
        .split("|")
        .map((c) => c.trim()),
    );

  if (rows.length === 0) return "";

  const firstRow = rows[0];
  const isHeader = rows.length >= 2;
  const dataRows = isHeader ? rows.slice(1) : rows;

  let html = '<table class="cm-lp-table">';
  if (isHeader) {
    html += "<thead><tr>";
    for (const cell of firstRow) {
      html += `<th>${escapeHtml(cell)}</th>`;
    }
    html += "</tr></thead>";
  }
  html += "<tbody>";
  for (const row of dataRows) {
    html += "<tr>";
    for (const cell of row) {
      html += `<td>${escapeHtml(cell)}</td>`;
    }
    html += "</tr>";
  }
  html += "</tbody></table>";
  return html;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

// ── Tag scanner ──

const TAG_RE = /(?:^|\s)#([\w-]+)/g;

function scanTags(
  ranges: Range<Decoration>[],
  doc: Text,
  activeLines: Set<number>,
) {
  const text = doc.toString();
  for (const m of text.matchAll(TAG_RE)) {
    const hashPos = m.index! + m[0].indexOf("#");
    const tagStart = hashPos + 1;
    const tagEnd = tagStart + m[1].length;
    const lineNum = doc.lineAt(hashPos).number;
    const isActive = activeLines.has(lineNum);

    // Don't decorate if it's a heading marker (# at very start of line)
    const line = doc.lineAt(hashPos);
    if (line.from === hashPos) continue;

    if (!isActive) {
      pushReplace(ranges, doc, hashPos, tagStart); // hide #
    }
    ranges.push(
      Decoration.mark({ class: "cm-lp-tag" }).range(hashPos, tagEnd),
    );
  }
}

// ── Core decoration builder ──

function buildDecorations(view: EditorView, isPaste = false): DecorationSet {
  const { state } = view;
  const { doc } = state;
  const ranges: Range<Decoration>[] = [];

  // Active lines = lines containing cursor or selection
  // Skip on paste so the entire pasted content renders as live preview
  const activeLines = new Set<number>();
  if (view.hasFocus && !isPaste) {
    for (const r of state.selection.ranges) {
      const first = doc.lineAt(r.from).number;
      const last = doc.lineAt(r.to).number;
      for (let n = first; n <= last; n++) activeLines.add(n);
    }
  }

  // Walk the entire syntax tree
  const tree = syntaxTree(state);

  tree.iterate({
    enter(node) {
      const { name, from: nFrom, to: nTo } = node;

      // ── FencedCode activation: cursor on any line activates whole block ──
      if (name === "FencedCode") {
        const firstLine = doc.lineAt(nFrom).number;
        const lastLine = doc.lineAt(Math.max(nFrom, nTo - 1)).number;
        let anyActive = false;
        for (let n = firstLine; n <= lastLine; n++) {
          if (activeLines.has(n)) {
            anyActive = true;
            break;
          }
        }
        if (anyActive) {
          for (let n = firstLine; n <= lastLine; n++) activeLines.add(n);
        }
      }

      // ── Table — replace with rendered HTML table widget ──
      if (name === "Table" && nFrom < nTo) {
        const firstLine = doc.lineAt(nFrom).number;
        const lastLine = doc.lineAt(Math.max(nFrom, nTo - 1)).number;
        let anyActive = false;
        for (let n = firstLine; n <= lastLine; n++) {
          if (activeLines.has(n)) {
            anyActive = true;
            break;
          }
        }
        if (!anyActive) {
          const tableText = doc.sliceString(nFrom, nTo);
          const html = buildTableHTML(tableText);
          if (html) {
            pushReplace(ranges, doc, nFrom, nTo, {
              widget: new TableWidget(html),
            });
            return false; // Skip child nodes
          }
        }
      }

      // ── Line-level classes (unconditional) ──
      const lineClass = LINE_CLASS_BY_BLOCK[name];
      if (lineClass) {
        const firstLine = doc.lineAt(nFrom);
        const lastLine = doc.lineAt(Math.max(nFrom, nTo - 1));
        for (let n = firstLine.number; n <= lastLine.number; n++) {
          const line = doc.line(n);
          ranges.push(Decoration.line({ class: lineClass }).range(line.from));
        }
      }

      // ── Inline mark classes (unconditional) ──
      const markClass = INLINE_MARK_CLASS[name];
      if (markClass && nFrom < nTo) {
        ranges.push(Decoration.mark({ class: markClass }).range(nFrom, nTo));
      }

      // ── Hide syntax tokens on inactive lines ──
      if (HIDEABLE_SYNTAX.has(name) && nFrom < nTo) {
        const lineNum = doc.lineAt(nFrom).number;
        if (!activeLines.has(lineNum)) {
          let hideTo = nTo;
          if (name === "HeaderMark" || name === "QuoteMark") {
            while (
              hideTo < doc.length &&
              doc.sliceString(hideTo, hideTo + 1) === " "
            ) {
              hideTo++;
            }
          }
          pushReplace(ranges, doc, nFrom, hideTo);
        }
      }

      // ── Horizontal rule ──
      if (name === "HorizontalRule") {
        const line = doc.lineAt(nFrom);
        if (!activeLines.has(line.number)) {
          ranges.push(
            Decoration.line({ class: "cm-lp-hr-line" }).range(line.from),
          );
          pushReplace(ranges, doc, line.from, line.to);
        }
      }

      // ── Image — render inline on inactive lines ──
      if (name === "Image" && nFrom < nTo) {
        const lineNum = doc.lineAt(nFrom).number;
        if (!activeLines.has(lineNum)) {
          const urlChild = node.node.getChild("URL");
          const url = urlChild ? doc.sliceString(urlChild.from, urlChild.to) : "";
          if (url) {
            pushReplace(ranges, doc, nFrom, nTo, {
              widget: new ImageWidget(url, ""),
            });
          } else {
            pushReplace(ranges, doc, nFrom, nTo);
          }
        }
      }

      // ── Lists — bullet widgets, ordered markers, hanging indent ──
      if (name === "ListMark" && nFrom < nTo) {
        const line = doc.lineAt(nFrom);
        const rawIndent = nFrom - line.from;
        const depth = Math.max(0, Math.floor(rawIndent / 2));
        const baseEm = 0.8;
        const alcoveEm = 1.2;
        const levelEm = 0.6;
        const padding = baseEm + alcoveEm + depth * levelEm;

        // Hanging indent layout
        ranges.push(
          Decoration.line({
            attributes: {
              style: `padding-left: ${padding}em; text-indent: -${alcoveEm}em`,
            },
          }).range(line.from),
        );

        // Trailing space after marker
        const hasTrailingSpace =
          nTo < doc.length && doc.sliceString(nTo, nTo + 1) === " ";
        const markEnd = hasTrailingSpace ? nTo + 1 : nTo;
        const markText = doc.sliceString(nFrom, nTo);

        // Check for task checkbox on this line
        const taskLead = line.text.match(/^(\s*[-*+]\s+)\[[ xX]\]/);
        const taskFrom =
          taskLead != null ? line.from + taskLead[1].length : undefined;

        if (taskFrom !== undefined) {
          // Hide `- ` (ListMark through the space before `[`)
          pushReplace(ranges, doc, nFrom, taskFrom);
        } else if (markText === "-" || markText === "*" || markText === "+") {
          // Bullet: substitute with widget
          pushReplace(ranges, doc, nFrom, markEnd, {
            widget: BULLET_WIDGET,
          });
        } else {
          // Ordered list: mark the number and hide trailing space
          ranges.push(
            Decoration.mark({ class: "cm-lp-list-marker" }).range(
              nFrom,
              nTo,
            ),
          );
          if (hasTrailingSpace) {
            pushReplace(ranges, doc, nTo, markEnd);
          }
        }
      }

      // ── Task checkboxes (GFM) ──
      if (name === "TaskMarker" && nFrom < nTo) {
        const markText = doc.sliceString(nFrom, nTo);
        const checked = /\[x\]/i.test(markText);
        const hasTrailingSpace =
          nTo < doc.length && doc.sliceString(nTo, nTo + 1) === " ";
        const replaceTo = hasTrailingSpace ? nTo + 1 : nTo;
        pushReplace(ranges, doc, nFrom, replaceTo, {
          widget: new TaskCheckboxWidget(checked),
        });
        if (checked) {
          const lineNum = doc.lineAt(nFrom).number;
          const lineEl = doc.line(lineNum);
          ranges.push(
            Decoration.line({ class: "cm-lp-task-done" }).range(lineEl.from),
          );
        }
      }
    },
  });

  // ── Tags (#tagname) ──
  scanTags(ranges, doc, activeLines);

  return Decoration.set(ranges, true);
}

// ── ViewPlugin ──

export const livePreviewPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }

    update(update: ViewUpdate) {
      if (
        update.docChanged ||
        update.selectionSet ||
        update.focusChanged
      ) {
        const isPaste = update.transactions.some((tr) => tr.isUserEvent("input.paste"));
        this.decorations = buildDecorations(update.view, isPaste);
      }
    }
  },
  {
    decorations: (v) => v.decorations,
  },
);
