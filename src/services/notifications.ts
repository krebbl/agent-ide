import { invoke, listen } from "./ipc";
import { useDevNotificationStore } from "../stores/devNotificationStore";
import { useTerminalStore } from "../stores/terminalStore";
import { showFaviconBadge } from "./faviconBadge";

export interface NotifyOptions {
  title: string;
  body: string;
  sessionId?: string;
}

export function initNotificationClickListener() {
  if (import.meta.env.VITE_TAURI === "true") {
    listen<{ sessionId: string }>("notification_clicked", (event) => {
      useTerminalStore.getState().focusSession(event.payload.sessionId);
    }).catch(() => {});
  }
}

let audioContext: AudioContext | null = null;

function playNotificationSound() {
  if (!audioContext) {
    const Ctx =
      window.AudioContext ||
      (
        window as unknown as {
          webkitAudioContext?: typeof AudioContext;
        }
      ).webkitAudioContext;
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

function showInAppToast(options: NotifyOptions) {
  useDevNotificationStore
    .getState()
    .addNotification(`${options.title}: ${options.body}`, options.sessionId);
  playNotificationSound();
}

async function showBrowserNotification(options: NotifyOptions) {
  if (typeof Notification === "undefined") return;

  if (Notification.permission === "denied") return;

  if (Notification.permission === "default") {
    const result = await Notification.requestPermission();
    if (result !== "granted") return;
  }

  const n = new Notification(options.title, { body: options.body });
  n.onclick = () => {
    n.close();
    window.focus();
    if (options.sessionId) {
      useTerminalStore.getState().focusSession(options.sessionId);
    }
  };
}

export function notify(options: NotifyOptions) {
  console.log("Sending notification:", options);
  showFaviconBadge();

  if (import.meta.env.VITE_TAURI === "true") {
    showInAppToast(options);
    invoke("notification_show", {
      title: options.title,
      body: options.body,
      sessionId: options.sessionId ?? null,
    }).catch((e) => {
      console.error("Failed to send notification:", e);
    });
    return;
  }

  showInAppToast(options);
  showBrowserNotification(options).catch((e) => {
    console.error("Failed to show browser notification:", e);
  });
}