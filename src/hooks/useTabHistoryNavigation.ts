import { useEffect } from "react";
import { useTerminalStore } from "../stores/terminalStore";

/** Syncs the webview's back/forward history (including the native macOS
 *  two-finger swipe gesture) to the terminal tab selection. */
export function useTabHistoryNavigation() {
  useEffect(() => {
    const onPopState = (e: PopStateEvent) => {
      useTerminalStore.getState().handleTabHistoryPop(e.state);
    };
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);
}
