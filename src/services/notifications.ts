import { invoke } from "@tauri-apps/api/core";
import { useDevNotificationStore } from "../stores/devNotificationStore";

export interface NotifyOptions {
  title: string;
  body: string;
  sessionId?: string;
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
  }).catch((e) => {
    console.error("Failed to send notification:", e);
  });
}
