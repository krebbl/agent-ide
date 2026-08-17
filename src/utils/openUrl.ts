import { invoke } from "../services/ipc";

export async function openUrl(url: string): Promise<void> {
  if (import.meta.env.VITE_TAURI === "true") {
    await invoke("util_open_url", { url });
  } else {
    window.open(url, "_blank");
  }
}
