import { invoke } from "@tauri-apps/api/core";
import { useDevNotificationStore } from "../stores/devNotificationStore";

export function notify(options: { title: string; body: string }) {
  console.log("Sending notification:", options);
  if (import.meta.env.DEV) {
    useDevNotificationStore.getState().addNotification(`${options.title}: ${options.body}`);
  }
  invoke("notification_show", {
    title: options.title,
    body: options.body,
  }).catch((e) => {
    console.error("Failed to send notification:", e);
  });
}
