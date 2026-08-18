import { useDevNotificationStore } from "../stores/devNotificationStore";

let badgeActive = false;
const iconBaseHref = "/icon.svg";

function setFavicon(href: string) {
  const link: HTMLLinkElement | null =
    document.querySelector('link[rel="icon"]');
  if (link) link.href = href;
}

let badgedDataUrl: string | null = null;

async function ensureBadgedIcon(): Promise<string> {
  if (badgedDataUrl) return badgedDataUrl;

  const size = 64;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d")!;

  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      ctx.clearRect(0, 0, size, size);
      ctx.drawImage(img, 0, 0, size, size);

      // Red dot badge at top-right corner
      const dotX = 52;
      const dotY = 12;
      const dotR = 10;
      ctx.beginPath();
      ctx.arc(dotX, dotY, dotR, 0, 2 * Math.PI);
      ctx.fillStyle = "#e03131";
      ctx.fill();
      ctx.strokeStyle = "#1e1e2e";
      ctx.lineWidth = 2.5;
      ctx.stroke();

      badgedDataUrl = canvas.toDataURL();
      resolve(badgedDataUrl);
    };
    img.onerror = () => {
      badgedDataUrl = iconBaseHref;
      resolve(badgedDataUrl);
    };
    img.src = iconBaseHref;
  });
}

export function showFaviconBadge() {
  if (badgeActive) return;
  badgeActive = true;

  ensureBadgedIcon().then((href) => {
    if (badgeActive) setFavicon(href);
  });
}

export function clearFaviconBadge() {
  if (!badgeActive) return;
  badgeActive = false;
  setFavicon(iconBaseHref);
}

export function initFaviconBadge() {
  // Auto-clear badge when tab gains focus and no toasts remain
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      const hasNotifications =
        useDevNotificationStore.getState().notifications.length > 0;
      if (!hasNotifications) clearFaviconBadge();
    }
  });

  // Auto-clear badge when last toast is dismissed
  useDevNotificationStore.subscribe((state) => {
    if (state.notifications.length === 0) clearFaviconBadge();
  });
}