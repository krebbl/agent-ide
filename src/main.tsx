import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { initConnectionStatusListener } from "./stores/connectionStatusStore";
import { initTerminalEventListeners } from "./services/terminalEvents";
import { initNotificationClickListener } from "./services/notifications";
import { useTerminalStore } from "./stores/terminalStore";
initConnectionStatusListener();
initNotificationClickListener();
initTerminalEventListeners()
  .then(() => useTerminalStore.getState().restoreSessions())
  .catch(() => {});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
