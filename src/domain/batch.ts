import type { CompressionPlan, MediaAnalysis, ProcessProgress, ProcessResult } from "./types";

export type BatchStatus =
  | "analyzing"
  | "waiting"
  | "processing"
  | "completed"
  | "noChange"
  | "failed"
  | "cancelled";

export type BatchItem = {
  id: string;
  path: string;
  status: BatchStatus;
  analysis: MediaAnalysis | null;
  plan: CompressionPlan | null;
  progress: ProcessProgress | null;
  result: ProcessResult | null;
  error: string | null;
};

export type BatchTotals = {
  files: number;
  completed: number;
  noChange: number;
  failed: number;
  cancelled: number;
  originalBytes: number;
  sendableBytes: number;
  savedBytes: number;
};

const terminalStatuses = new Set<BatchStatus>(["completed", "noChange", "failed", "cancelled"]);

export function isTerminal(status: BatchStatus): boolean {
  return terminalStatuses.has(status);
}

export function batchTotals(items: BatchItem[]): BatchTotals {
  const originalBytes = items.reduce((sum, item) => sum + (item.analysis?.sizeBytes ?? 0), 0);
  const sendableBytes = items.reduce((sum, item) => {
    if (item.status !== "completed" && item.status !== "noChange") return sum;
    return sum + (item.result?.outputBytes ?? item.analysis?.sizeBytes ?? 0);
  }, 0);
  const successfulOriginalBytes = items.reduce((sum, item) => {
    if (item.status !== "completed" && item.status !== "noChange") return sum;
    return sum + (item.analysis?.sizeBytes ?? 0);
  }, 0);
  return {
    files: items.length,
    completed: items.filter((item) => item.status === "completed").length,
    noChange: items.filter((item) => item.status === "noChange").length,
    failed: items.filter((item) => item.status === "failed").length,
    cancelled: items.filter((item) => item.status === "cancelled").length,
    originalBytes,
    sendableBytes,
    savedBytes: Math.max(0, successfulOriginalBytes - sendableBytes),
  };
}

export function overallProgress(items: BatchItem[]): number {
  if (items.length === 0) return 0;
  const terminal = items.filter((item) => isTerminal(item.status)).length;
  const activeFraction = items
    .filter((item) => item.status === "processing")
    .reduce((sum, item) => sum + (item.progress?.percent ?? 0) / 100, 0);
  return Math.round(((terminal + activeFraction) / items.length) * 100);
}

export function primaryActionLabel(count: number): string {
  return `Optimize ${count} ${count === 1 ? "file" : "files"}`;
}

export function statusLabel(status: BatchStatus): string {
  const labels: Record<BatchStatus, string> = {
    analyzing: "Reading file",
    waiting: "Ready",
    processing: "Optimizing",
    completed: "Ready to send",
    noChange: "No change needed",
    failed: "Needs attention",
    cancelled: "Cancelled",
  };
  return labels[status];
}

export function pathKey(path: string): string {
  return path.replace(/\\/g, "/").toLocaleLowerCase();
}
