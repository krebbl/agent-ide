import type { ITerminalAddon, Terminal } from "@xterm/xterm";

// Works around https://github.com/xtermjs/xterm.js/issues/5894: on macOS
// WKWebView, a dead key (e.g. Option+N for ~) followed by a non-combining
// key duplicates the dead char and drops the next key. Remove once fixed
// upstream.
export class WebKitDeadKeyAddon implements ITerminalAddon {
  private _deadKeyDownSeen = false;
  private _commit: string | null = null;
  private _wasDead = false;
  private _textarea: HTMLTextAreaElement | null = null;
  private readonly _emit: (data: string) => void;

  constructor(emit: (data: string) => void) {
    this._emit = emit;
  }

  activate(term: Terminal): void {
    this._textarea = term.textarea ?? null;
    this._textarea?.addEventListener("keydown", this._onKeyDown, true);
    this._textarea?.addEventListener("compositionstart", this._onCompositionStart, true);
    this._textarea?.addEventListener("compositionend", this._onCompositionEnd, true);
  }

  dispose(): void {
    this._textarea?.removeEventListener("keydown", this._onKeyDown, true);
    this._textarea?.removeEventListener("compositionstart", this._onCompositionStart, true);
    this._textarea?.removeEventListener("compositionend", this._onCompositionEnd, true);
    this._textarea = null;
    this._deadKeyDownSeen = false;
    this._commit = null;
    this._wasDead = false;
  }

  handle(e: KeyboardEvent): boolean {
    if (
      e.type === "keypress" &&
      this._wasDead &&
      this._commit !== null &&
      e.charCode === this._commit.charCodeAt(0)
    ) {
      this._commit = null;
      this._wasDead = false;
      return true;
    }
    if (
      e.type === "keydown" &&
      this._wasDead &&
      this._commit !== null &&
      e.key.length === 2 &&
      e.key[0] === this._commit
    ) {
      const data = e.key.slice(1);
      setTimeout(() => this._emit(data), 0);
      return true;
    }
    return false;
  }

  private _onKeyDown = (e: Event): void => {
    const ke = e as KeyboardEvent;
    if (ke.key === "Dead" || ke.key === "AltGraph") {
      this._deadKeyDownSeen = true;
    }
  };

  private _onCompositionStart = (): void => {
    this._commit = null;
    this._wasDead = false;
    this._deadKeyDownSeen = false;
  };

  private _onCompositionEnd = (e: Event): void => {
    const data = (e as CompositionEvent).data;
    this._commit = data || null;
    this._wasDead = this._deadKeyDownSeen;
    this._deadKeyDownSeen = false;
  };
}
