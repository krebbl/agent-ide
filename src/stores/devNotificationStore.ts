import { create } from "zustand";

export interface DevNotification {
  id: string;
  message: string;
}

interface DevNotificationStore {
  notifications: DevNotification[];
  addNotification: (message: string) => void;
  removeNotification: (id: string) => void;
}

export const useDevNotificationStore = create<DevNotificationStore>((set) => ({
  notifications: [],

  addNotification: (message) => {
    const id = crypto.randomUUID();
    set((state) => ({
      notifications: [...state.notifications, { id, message }],
    }));
    setTimeout(() => {
      set((state) => ({
        notifications: state.notifications.filter((n) => n.id !== id),
      }));
    }, 5000);
  },

  removeNotification: (id) =>
    set((state) => ({
      notifications: state.notifications.filter((n) => n.id !== id),
    })),
}));
