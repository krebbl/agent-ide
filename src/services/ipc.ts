export interface IpcEvent<T = unknown> {
  payload: T;
}

interface IpcImpl {
  invoke<T>(command: string, payload?: Record<string, unknown>): Promise<T>;
  listen<T>(
    event: string,
    handler: (event: IpcEvent<T>) => void,
  ): Promise<() => void>;
}

let implPromise: Promise<IpcImpl> | null = null;

function getImpl(): Promise<IpcImpl> {
  if (!implPromise) {
    implPromise = (
      import.meta.env.VITE_TAURI === "true"
        ? import("./ipc/tauri")
        : import("./ipc/web")
    ) as Promise<IpcImpl>;
  }
  return implPromise;
}

export async function invoke<T>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  const impl = await getImpl();
  return impl.invoke<T>(command, payload);
}

export function listen<T>(
  event: string,
  handler: (event: IpcEvent<T>) => void,
): Promise<() => void> {
  return getImpl().then((impl) => impl.listen<T>(event, handler));
}
