import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { allItemsTerminal, batchTotals, cancelWaitingItems, overallProgress, pathKey, type BatchItem } from "../domain/batch";
import { outputDefaultPath } from "../domain/format";
import type { CompressionPlan, CompressionStrategy, MediaAnalysis, ProcessProgress, ProcessResult } from "../domain/types";

type UseBatchQueueOptions = {
  targetBytes: number;
  strategy: CompressionStrategy;
};

function newItem(path: string): BatchItem {
  return {
    id: crypto.randomUUID(),
    path,
    status: "analyzing",
    analysis: null,
    plan: null,
    progress: null,
    result: null,
    error: null,
    allocationBytes: null,
  };
}

export function useBatchQueue({ targetBytes, strategy }: UseBatchQueueOptions) {
  const [items, setItems] = useState<BatchItem[]>([]);
  const [running, setRunning] = useState(false);
  const itemsRef = useRef(items);
  const cancelRequested = useRef(false);
  const activeJobId = useRef<string | null>(null);
  const activeItemId = useRef<string | null>(null);

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  const patchItem = useCallback((id: string, patch: Partial<BatchItem>) => {
    setItems((current) => current.map((item) => item.id === id ? { ...item, ...patch } : item));
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let removeListener: (() => void) | undefined;
    listen<ProcessProgress>("fitsend://process-progress", (event) => {
      if (disposed || event.payload.jobId !== activeJobId.current || !activeItemId.current) return;
      patchItem(activeItemId.current, { progress: event.payload });
    }).then((unlisten) => {
      if (disposed) unlisten();
      else removeListener = unlisten;
    }).catch(() => undefined);
    return () => {
      disposed = true;
      removeListener?.();
    };
  }, [patchItem]);

  const addPaths = useCallback(async (paths: string[]) => {
    const known = new Set(itemsRef.current.map((item) => pathKey(item.path)));
    const additions = paths.filter((path) => {
      const key = pathKey(path);
      if (known.has(key)) return false;
      known.add(key);
      return true;
    }).map(newItem);
    if (additions.length === 0) return;
    setItems((current) => [...current, ...additions]);
    for (const item of additions) {
      try {
        const analysis = await invoke<MediaAnalysis>("analyze_media", { path: item.path });
        patchItem(item.id, { analysis, status: "waiting" });
      } catch (reason) {
        patchItem(item.id, { status: "failed", error: String(reason) });
      }
    }
  }, [patchItem]);

  const removeItem = useCallback((id: string) => {
    if (running) return;
    setItems((current) => current.filter((item) => item.id !== id));
  }, [running]);

  const clear = useCallback(() => {
    if (!running) setItems([]);
  }, [running]);

  const start = useCallback(async () => {
    if (running || targetBytes < 8 * 1024) return;
    cancelRequested.current = false;
    setRunning(true);
    const runnable = itemsRef.current.filter((item) => item.status === "waiting" && item.analysis);
    for (const snapshot of runnable) {
      if (cancelRequested.current) break;
      const analysis = snapshot.analysis as MediaAnalysis;
      try {
        const plan = await invoke<CompressionPlan>("build_plan", {
          request: { analysis, targetBytes, strategy },
        });
        patchItem(snapshot.id, { plan });
        if (!plan.feasible) {
          patchItem(snapshot.id, {
            status: "failed",
            error: plan.warnings[0] ?? "This file cannot meet the selected limit.",
          });
          continue;
        }
        const jobId = crypto.randomUUID();
        activeJobId.current = jobId;
        activeItemId.current = snapshot.id;
        patchItem(snapshot.id, {
          status: "processing",
          error: null,
          progress: { jobId, percent: 0, stage: "Starting local processing", encodedSeconds: null, attempt: 1 },
        });
        const nextResult = await invoke<ProcessResult>("process_media", {
          jobId,
          request: {
            analysis,
            targetBytes,
            outputPath: outputDefaultPath(analysis.path, plan.outputExtension),
            strategy,
          },
        });
        patchItem(snapshot.id, {
          status: nextResult.outcome === "noChange" ? "noChange" : "completed",
          result: nextResult,
          progress: null,
        });
      } catch (reason) {
        const message = String(reason);
        patchItem(snapshot.id, {
          status: message.includes("PROCESS_CANCELLED") ? "cancelled" : "failed",
          error: message.includes("PROCESS_CANCELLED") ? null : message,
          progress: null,
        });
      } finally {
        activeJobId.current = null;
        activeItemId.current = null;
      }
    }
    if (cancelRequested.current) {
      setItems(cancelWaitingItems);
    }
    setRunning(false);
  }, [patchItem, running, strategy, targetBytes]);

  const cancel = useCallback(async () => {
    cancelRequested.current = true;
    const jobId = activeJobId.current;
    if (jobId) {
      try {
        await invoke<boolean>("cancel_process", { jobId });
      } catch {
        // The active invocation will surface its own error and the queue still stops.
      }
    }
  }, []);

  const totals = useMemo(() => batchTotals(items), [items]);
  const progress = useMemo(() => overallProgress(items), [items]);
  const hasRunnable = items.some((item) => item.status === "waiting");
  const allTerminal = allItemsTerminal(items);

  return {
    items,
    running,
    totals,
    progress,
    hasRunnable,
    allTerminal,
    addPaths,
    removeItem,
    clear,
    start,
    cancel,
  };
}
