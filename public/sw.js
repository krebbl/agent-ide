self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(clients.claim());
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  event.waitUntil(Promise.resolve());
});