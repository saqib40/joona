import { type DependencyList, useEffect, useState } from "react";

/** Runs `load` whenever `deps` change and keeps the last good result while reloading. */
export function useLoad<T>(load: () => Promise<T>, deps: DependencyList): { data: T | null; error: string | null } {
  const [state, setState] = useState<{ data: T | null; error: string | null }>({ data: null, error: null });
  useEffect(() => {
    let live = true;
    load().then(
      (data) => live && setState({ data, error: null }),
      (e) => live && setState((s) => ({ data: s.data, error: String(e) })),
    );
    return () => {
      live = false;
    };
    // `load` is a new function on every render; `deps` says when it really changes.
  }, deps);
  return state;
}
