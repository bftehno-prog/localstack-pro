import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../ui/api";
import type { AppRunResult, AppSnapshot, OperationEntry } from "../ui/types";

type RunOptions = {
  silent?: boolean;
  label?: string;
  successLabel?: string;
  serial?: boolean;
};

export function useAppState() {
  const [state, setState] = useState<AppSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busyCount, setBusyCount] = useState(0);
  const [actionLabel, setActionLabel] = useState<string | null>(null);
  const [operations, setOperations] = useState<OperationEntry[]>([]);
  const queueRef = useRef<Promise<void>>(Promise.resolve());
  const lastActionRef = useRef<{ action: () => Promise<AppRunResult>; options?: RunOptions } | null>(null);
  const busyRef = useRef(false);
  const busy = busyCount > 0;

  useEffect(() => {
    busyRef.current = busy;
  }, [busy]);

  useEffect(() => {
    if (!error) return;
    const timer = window.setTimeout(() => setError(null), 8000);
    return () => window.clearTimeout(timer);
  }, [error]);
  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 5000);
    return () => window.clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    const capture = (event: ErrorEvent | PromiseRejectionEvent) => {
      const message = "reason" in event ? String(event.reason) : event.message;
      localStorage.setItem("localstack.lastCrash", JSON.stringify({ message, at: new Date().toISOString() }));
    };
    window.addEventListener("error", capture);
    window.addEventListener("unhandledrejection", capture);
    return () => {
      window.removeEventListener("error", capture);
      window.removeEventListener("unhandledrejection", capture);
    };
  }, []);

  const refresh = useCallback(async (silent = false) => {
    try {
      const next = await api.getState();
      startTransition(() => setState(next));
    } catch (err) {
      if (!silent) setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const execute = useCallback(async (action: () => Promise<AppRunResult>, options?: RunOptions) => {
    const silent = options?.silent === true;
    const label = options?.label ?? "Action in progress...";
    const operationId = crypto.randomUUID();
    const startedAt = Date.now();
    try {
      if (!silent) {
        setBusyCount((count) => count + 1);
        setActionLabel(label);
        const operation: OperationEntry = {
          id: operationId,
          label,
          status: "running",
          startedAt: new Date(startedAt).toISOString()
        };
        setOperations((items) => [operation, ...items].slice(0, 12));
      }
      setError(null);
      setNotice(null);
      const result = await action();
      if (result && typeof result === "object" && "services" in result && Array.isArray(result.services)) {
        startTransition(() => setState(result as AppSnapshot));
      }
      if (!silent) {
        const finishedAt = Date.now();
        setOperations((items) => items.map((item) => item.id === operationId ? {
          ...item,
          status: "success",
          finishedAt: new Date(finishedAt).toISOString(),
          durationMs: finishedAt - startedAt,
          message: options?.successLabel
        } : item));
        if (options?.successLabel && localStorage.getItem("localstack.notificationLevel") !== "Errors only") {
          setNotice(options.successLabel);
        }
      }
      return result;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      if (!silent) {
        const finishedAt = Date.now();
        setOperations((items) => items.map((item) => item.id === operationId ? {
          ...item,
          status: "error",
          finishedAt: new Date(finishedAt).toISOString(),
          durationMs: finishedAt - startedAt,
          message
        } : item));
      }
      throw err;
    } finally {
      if (!silent) {
        setBusyCount((count) => Math.max(0, count - 1));
        setActionLabel((current) => current === label ? null : current);
      }
    }
  }, []);

  const run = useCallback((action: () => Promise<AppRunResult>, options?: RunOptions) => {
    if (!options?.silent) lastActionRef.current = { action, options };
    if (options?.silent) return execute(action, options);
    if (options?.serial === false) return execute(action, options);

    const next = queueRef.current
      .catch(() => undefined)
      .then(() => execute(action, options));
    queueRef.current = next.then(() => undefined, () => undefined);
    return next;
  }, [execute]);

  const retryLast = useCallback(() => {
    if (!lastActionRef.current) return Promise.resolve();
    return run(lastActionRef.current.action, lastActionRef.current.options);
  }, [run]);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (!document.hidden && !busyRef.current) void refresh(true);
    }, window.location.hash.replace(/^#/, "") === "tray" ? 15000 : 30000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  return useMemo(() => ({ state, loading, error, notice, busy, actionLabel, operations, refresh, run, retryLast, setError }), [state, loading, error, notice, busy, actionLabel, operations, refresh, run, retryLast]);
}
