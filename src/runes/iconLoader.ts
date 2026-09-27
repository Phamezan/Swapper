/** Icon bytes are resolved once per key and shared across every component.
 *  Concurrent callers for the same key await the same in-flight request instead
 *  of each firing its own IPC call, which is what made a pro-builds page send
 *  dozens of duplicate requests at once. */
const resolved = new Map<string, string>();
const inflight = new Map<string, Promise<string>>();

/** The already-resolved data URL for a key, for a component's first render. */
export function cachedIcon(key: string): string | undefined {
  return resolved.get(key);
}

/** Resolves an icon, de-duplicating concurrent callers for the same key. */
export function loadIcon(key: string, resolve: () => Promise<string>): Promise<string> {
  const cached = resolved.get(key);
  if (cached) return Promise.resolve(cached);
  const pending = inflight.get(key);
  if (pending) return pending;
  const promise = resolve()
    .then((data) => {
      resolved.set(key, data);
      return data;
    })
    .finally(() => {
      inflight.delete(key);
    });
  inflight.set(key, promise);
  return promise;
}
