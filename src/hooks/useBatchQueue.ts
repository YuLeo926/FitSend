import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { allItemsTerminal, batchProof, batchTotals, cancelWaitingItems, failWaitingItems, isBatchAnalysisComplete, overallProgress, pathKey, type BatchItem } from "../domain/batch";
import { applyBudgetAllocations, budgetRequest } from "../domain/budget";
import { outputDefaultPath } from "../domain/format";
import type { BatchBudget, CompressionPlan, CompressionStrategy, DestinationRule, MediaAnalysis, ProcessProgress, ProcessResult } from "../domain/types";

type UseBatchQueueOptions = {
  rule: DestinationRule;
  strategy: CompressionStrategy;
};

const INITIAL_BUDGET_ERROR = "FitSend could not allocate the selected limit across this batch.";
const REBALANCE_ERROR = "FitSend could not allocate the remaining limit across the waiting files.";

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

export function useBatchQueue({ rule, strategy }: UseBatchQueueOptions) {
  const [items, setItems] = useState<BatchItem[]>([]);
  const [running, setRunning] = useState(false);
  const itemsRef = useRef(items);
  const cancelRequested = useRef(false);
  const activeJobId = useRef<string | null>(null);
  const activeItemId = useRef<string | null>(null);

  const replaceItems = useCallback((transform: (current: BatchItem[]) => BatchItem[]) => {
    const next = transform(itemsRef.current);
    itemsRef.current = next;
    setItems(next);
    return next;
  }, []);

  const patchItem = useCallback((id: string, patch: Partial<BatchItem>) => {
    replaceItems((current) => current.map((item) => item.id === id ? { ...item, ...patch } : item));
  }, [replaceItems]);

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
    replaceItems((current) => [...current, ...additions]);
    for (const item of additions) {
      try {
        const analysis = await invoke<MediaAnalysis>("analyze_media", { path: item.path });
        patchItem(item.id, { analysis, status: "waiting" });
      } catch (reason) {
        patchItem(item.id, { status: "failed", error: String(reason) });
      }
    }
  }, [patchItem, replaceItems]);

  const removeItem = useCallback((id: string) => {
    if (running) return;
    replaceItems((current) => current.filter((item) => item.id !== id));
  }, [replaceItems, running]);

  const clear = useCallback(() => {
    if (!running) replaceItems(() => []);
  }, [replaceItems, running]);

  const start = useCallback(async () => {
    const currentItems = itemsRef.current;
    if (running || rule.maxBytes < 8 * 1024 || !isBatchAnalysisComplete(currentItems)) return;
    const waiting = currentItems.filter((item) => item.status === "waiting" && item.analysis);
    if (waiting.length === 0) return;
    cancelRequested.current = false;
    setRunning(true);
    try {
      let initial: BatchBudget;
      try {
        initial = await invoke<BatchBudget>("build_batch_budget", {
          request: budgetRequest(itemsRef.current, rule, false),
        });
      } catch (reason) {
        if (cancelRequested.current) return;
        replaceItems((current) => failWaitingItems(current, `${INITIAL_BUDGET_ERROR} ${String(reason)}`));
        return;
      }

      if (cancelRequested.current) return;
      if (!initial.feasible) {
        replaceItems((current) => failWaitingItems(current, initial.reason ?? INITIAL_BUDGET_ERROR));
        return;
      }

      try {
        replaceItems((current) => applyBudgetAllocations(current, initial));
      } catch (reason) {
        replaceItems((current) => failWaitingItems(current, `${INITIAL_BUDGET_ERROR} ${String(reason)}`));
        return;
      }
      if (itemsRef.current.some((item) => item.status === "waiting" && item.allocationBytes === null)) {
        replaceItems((current) => failWaitingItems(current, INITIAL_BUDGET_ERROR));
        return;
      }

      const runnable = itemsRef.current.filter((item) => item.status === "waiting" && item.analysis);
      for (const snapshot of runnable) {
        if (cancelRequested.current) break;
        const liveItem = itemsRef.current.find((item) => item.id === snapshot.id);
        if (!liveItem || liveItem.status !== "waiting") continue;
        if (!liveItem.analysis || liveItem.allocationBytes === null) {
          replaceItems((current) => failWaitingItems(current, INITIAL_BUDGET_ERROR));
          break;
        }

        const analysis = liveItem.analysis as MediaAnalysis;
        const targetBytes = liveItem.allocationBytes;
        try {
          const plan = await invoke<CompressionPlan>("build_plan", {
            request: { analysis, targetBytes, strategy },
          });
          if (cancelRequested.current) break;
          patchItem(snapshot.id, { plan });
          if (!plan.feasible) {
            patchItem(snapshot.id, {
              status: "failed",
              error: plan.warnings[0] ?? "This file cannot meet the selected limit.",
            });
          } else {
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
          }
        } catch (reason) {
          const message = String(reason);
          if (cancelRequested.current) {
            if (activeJobId.current) {
              patchItem(snapshot.id, { status: "cancelled", error: null, progress: null });
            }
          } else {
            patchItem(snapshot.id, {
              status: message.includes("PROCESS_CANCELLED") ? "cancelled" : "failed",
              error: message.includes("PROCESS_CANCELLED") ? null : message,
              progress: null,
            });
          }
        } finally {
          activeJobId.current = null;
          activeItemId.current = null;
        }

        if (cancelRequested.current) break;
        const remaining = itemsRef.current.filter((item) => item.status === "waiting");
        if (remaining.length === 0) continue;

        let nextBudget: BatchBudget;
        try {
          nextBudget = await invoke<BatchBudget>("rebalance_batch_budget", {
            request: budgetRequest(itemsRef.current, rule, true),
          });
        } catch (reason) {
          if (cancelRequested.current) break;
          replaceItems((current) => failWaitingItems(current, `${REBALANCE_ERROR} ${String(reason)}`));
          break;
        }

        if (cancelRequested.current) break;
        if (!nextBudget.feasible) {
          replaceItems((current) => failWaitingItems(current, nextBudget.reason ?? REBALANCE_ERROR));
          break;
        }
        try {
          replaceItems((current) => applyBudgetAllocations(current, nextBudget));
        } catch (reason) {
          replaceItems((current) => failWaitingItems(current, `${REBALANCE_ERROR} ${String(reason)}`));
          break;
        }
      }
    } finally {
      activeJobId.current = null;
      activeItemId.current = null;
      if (cancelRequested.current) {
        replaceItems(cancelWaitingItems);
      }
      setRunning(false);
    }
  }, [patchItem, replaceItems, rule, running, strategy]);

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

  const totals = useMemo(() => batchTotals(items, rule), [items, rule]);
  const proof = useMemo(() => batchProof(items, rule), [items, rule]);
  const progress = useMemo(() => overallProgress(items), [items]);
  const hasRunnable = items.some((item) => item.status === "waiting");
  const allTerminal = allItemsTerminal(items);

  return {
    items,
    running,
    totals,
    proof,
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
