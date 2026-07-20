import { invoke } from "@tauri-apps/api/core";
import { useDevNotificationStore } from "../stores/devNotificationStore";

export interface NotifyOptions {
  title: string;
  body: string;
  sessionId?: string;
}

export function notify(options: NotifyOptions) {
  console.log("Sending notification:", options);
  if (import.meta.env.DEV) {
    useDevNotificationStore
      .getState()
      .addNotification(`${options.title}: ${options.body}`, options.sessionId);
  }
  invoke("notification_show", {
    title: options.title,
    body: options.body,
  }).catch((e) => {
    console.error("Failed to send notification:", e);
  });
}
