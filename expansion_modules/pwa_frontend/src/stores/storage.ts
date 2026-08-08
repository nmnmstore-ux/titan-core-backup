// Tiny localStorage-backed store.

function read<T>(key: string): T | null {
  const raw = localStorage.getItem(key);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

function write<T>(key: string, value: T): void {
  localStorage.setItem(key, JSON.stringify(value));
}

function remove(key: string): void {
  localStorage.removeItem(key);
}

export function create<T>(key: string) {
  return {
    get: (): T | null => read<T>(key),
    set: (value: T) => write(key, value),
    remove: () => remove(key),
  };
}
