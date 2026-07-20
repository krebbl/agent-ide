import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useDevNotificationStore } from "../stores/devNotificationStore";
import { useTerminalStore } from "../stores/terminalStore";

export interface NotifyOptions {
  title: string;
  body: string;
  sessionId?: string;
}

export function initNotificationClickListener() {
  listen<{ sessionId: string }>("notification_clicked", (event) => {
    useTerminalStore.getState().focusSession(event.payload.sessionId);
  }).catch(() => {});
}

let audioContext: AudioContext | null = null;

function playNotificationSound() {
  if (!audioContext) {
    const Ctx = window.AudioContext || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) return;
    audioContext = new Ctx();
  }

  if (audioContext.state === "suspended") {
    audioContext.resume().catch(() => {});
  }

  const osc = audioContext.createOscillator();
  const gain = audioContext.createGain();

  osc.type = "sine";
  osc.frequency.setValueAtTime(880, audioContext.currentTime);
  osc.frequency.exponentialRampToValueAtTime(440, audioContext.currentTime + 0.1);

  gain.gain.setValueAtTime(0.1, audioContext.currentTime);
  gain.gain.exponentialRampToValueAtTime(0.001, audioContext.currentTime + 0.2);

  osc.connect(gain);
  gain.connect(audioContext.destination);
  osc.start();
  osc.stop(audioContext.currentTime + 0.2);
}

export function notify(options: NotifyOptions) {
  console.log("Sending notification:", options);
  if (import.meta.env.DEV) {
    useDevNotificationStore
      .getState()
      .addNotification(`${options.title}: ${options.body}`, options.sessionId);
    playNotificationSound();
  }
  invoke("notification_show", {
    title: options.title,
    body: options.body,
    sessionId: options.sessionId ?? null,
  }).catch((e) => {
    console.error("Failed to send notification:", e);
  });
}
