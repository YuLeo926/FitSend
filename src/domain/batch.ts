import { formatBytes } from "./format";
import type { CompressionPlan, DestinationRule, LimitScope, MediaAnalysis, ProcessProgress, ProcessResult } from "./types";

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
  allocationBytes: number | null;
};

export type BatchTotals = {
  files: number;
  completed: number;
  noChange: number;
  failed: number;
  cancelled: number;
  originalBytes: number;
  acceptedFiles: number;
  acceptedBytes: number;
  sendableBytes: number;
  savedBytes: number;
  limitScope: LimitScope;
  workingCeilingBytes: number;
  allSelectedAccepted: boolean;
  proofValid: boolean;
};

export type BatchProof = {
  scope: LimitScope;
  valid: boolean;
  allSelectedAccepted: boolean;
  acceptedFiles: number;
  acceptedBytes: number;
  ceilingBytes: number;
  attentionFiles: number;
  tone: "success" | "partial" | "invalid";
  headline: string;
  detail: string;
};

export type BatchBadgeState = {
  text: string;
  tone: "good" | "partial" | "invalid" | "warning";
};

const terminalStatuses = new Set<BatchStatus>(["completed", "noChange", "failed", "cancelled"]);

export function isTerminal(status: BatchStatus): boolean {
  return terminalStatuses.has(status);
}

export function cancelWaitingItems(items: BatchItem[]): BatchItem[] {
  return items.map((item) => item.status === "waiting" ? { ...item, status: "cancelled" } : item);
}

export function failWaitingItems(items: BatchItem[], message: string): BatchItem[] {
  return items.map((item) => item.status === "waiting"
    ? { ...item, status: "failed", error: message, progress: null }
    : item);
}

export function allItemsTerminal(items: BatchItem[]): boolean {
  return items.length > 0 && items.every((item) => isTerminal(item.status));
}

export function isBatchAnalysisComplete(items: BatchItem[]): boolean {
  return items.length > 0 && items.every((item) => item.status !== "analyzing");
}

export function isBatchConfigurationLocked(items: BatchItem[]): boolean {
  return items.some((item) => item.allocationBytes !== null || item.plan !== null || item.result !== null);
}

function isAcceptedStatus(item: BatchItem): boolean {
  return item.status === "completed" || item.status === "noChange";
}

function acceptedByteValue(item: BatchItem): { bytes: number; verifiable: boolean } | null {
  if (item.status === "completed") {
    const bytes = item.result?.outputBytes;
    return {
      bytes: Number.isFinite(bytes) && (bytes ?? -1) >= 0 ? bytes! : 0,
      verifiable: item.result?.verified === true && Number.isFinite(bytes) && (bytes ?? -1) >= 0,
    };
  }
  if (item.status === "noChange") {
    const bytes = item.result?.outputBytes ?? item.analysis?.sizeBytes;
    return {
      bytes: Number.isFinite(bytes) && (bytes ?? -1) >= 0 ? bytes! : 0,
      verifiable: item.result?.verified !== false && Number.isFinite(bytes) && (bytes ?? -1) >= 0,
    };
  }
  return null;
}

type ProofBasis = {
  acceptedItems: Array<{ item: BatchItem; bytes: number; verifiable: boolean }>;
  acceptedBytes: number;
  allSelectedAccepted: boolean;
  valid: boolean;
};

function proofBasis(items: BatchItem[], rule: DestinationRule): ProofBasis {
  const acceptedItems = items.flatMap((item) => {
    const value = acceptedByteValue(item);
    return value ? [{ item, ...value }] : [];
  });
  const acceptedBytes = acceptedItems.reduce((sum, accepted) => sum + accepted.bytes, 0);
  const hasValidCeiling = Number.isFinite(rule.maxBytes) && rule.maxBytes >= 0;
  const hasVerifiableAcceptedRows = acceptedItems.length > 0 && acceptedItems.every(({ verifiable }) => verifiable);
  const withinScope = rule.scope === "perFile"
    ? acceptedItems.every(({ bytes }) => bytes <= rule.maxBytes)
    : acceptedBytes <= rule.maxBytes;
  return {
    acceptedItems,
    acceptedBytes,
    allSelectedAccepted: items.length > 0 && items.every(isAcceptedStatus),
    valid: hasValidCeiling && hasVerifiableAcceptedRows && withinScope,
  };
}

function fileCount(count: number): string {
  return `${count} ${count === 1 ? "file" : "files"}`;
}

function acceptedFileSubject(count: number): string {
  return `${count} accepted ${count === 1 ? "file" : "files"}`;
}

function attentionClause(count: number): string {
  if (count === 0) return "";
  return `; ${fileCount(count)} ${count === 1 ? "needs" : "need"} attention`;
}

export function batchProof(items: BatchItem[], rule: DestinationRule): BatchProof {
  const basis = proofBasis(items, rule);
  const acceptedFiles = basis.acceptedItems.length;
  const attentionFiles = items.length - acceptedFiles;
  const ceiling = formatBytes(rule.maxBytes);
  const attention = attentionClause(attentionFiles);
  const headline = rule.scope === "perFile"
    ? basis.valid
      ? `${acceptedFileSubject(acceptedFiles)} ${acceptedFiles === 1 ? "is" : "are each"} under ${ceiling}${attention}`
      : `${acceptedFileSubject(acceptedFiles)} ${acceptedFiles === 1 ? "could not be verified" : "could not all be verified"} under ${ceiling}${attention}`
    : basis.valid
      ? `${acceptedFileSubject(acceptedFiles)} total ${formatBytes(basis.acceptedBytes)} — verified under ${ceiling}${attention}`
      : `${acceptedFileSubject(acceptedFiles)} total ${formatBytes(basis.acceptedBytes)} — not verified under ${ceiling}${attention}`;
  const tone = !basis.valid ? "invalid" : basis.allSelectedAccepted ? "success" : "partial";
  const detail = tone === "success"
    ? "All selected files are covered by this proof. Originals remain untouched."
    : tone === "partial"
      ? "Only the accepted files are covered by this proof. Files needing attention are not included."
      : "The accepted output does not satisfy the selected limit. Do not treat this batch as ready to send.";
  return {
    scope: rule.scope,
    valid: basis.valid,
    allSelectedAccepted: basis.allSelectedAccepted,
    acceptedFiles,
    acceptedBytes: basis.acceptedBytes,
    ceilingBytes: rule.maxBytes,
    attentionFiles,
    tone,
    headline,
    detail,
  };
}

export function batchTotals(items: BatchItem[], rule: DestinationRule): BatchTotals {
  const basis = proofBasis(items, rule);
  const originalBytes = items.reduce((sum, item) => sum + (item.analysis?.sizeBytes ?? 0), 0);
  const successfulOriginalBytes = items.reduce((sum, item) => {
    if (!isAcceptedStatus(item)) return sum;
    return sum + (item.analysis?.sizeBytes ?? 0);
  }, 0);
  return {
    files: items.length,
    completed: items.filter((item) => item.status === "completed").length,
    noChange: items.filter((item) => item.status === "noChange").length,
    failed: items.filter((item) => item.status === "failed").length,
    cancelled: items.filter((item) => item.status === "cancelled").length,
    originalBytes,
    acceptedFiles: basis.acceptedItems.length,
    acceptedBytes: basis.acceptedBytes,
    sendableBytes: basis.acceptedBytes,
    savedBytes: Math.max(0, successfulOriginalBytes - basis.acceptedBytes),
    limitScope: rule.scope,
    workingCeilingBytes: rule.maxBytes,
    allSelectedAccepted: basis.allSelectedAccepted,
    proofValid: basis.valid,
  };
}

export function batchBadgeState(
  proof: BatchProof,
  validTarget: boolean,
  allTerminal: boolean,
  readyCount: number,
): BatchBadgeState {
  if (!validTarget) return { text: "Check limit", tone: "warning" };
  if (!allTerminal) return { text: `${readyCount} to fit`, tone: "good" };
  if (proof.tone === "success") return { text: `${proof.acceptedFiles} verified`, tone: "good" };
  if (proof.tone === "partial") return { text: `${proof.acceptedFiles} accepted`, tone: "partial" };
  return { text: "Verification failed", tone: "invalid" };
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
  return `Make ${count} ${count === 1 ? "file" : "files"} fit`;
}

export function statusLabel(status: BatchStatus): string {
  const labels: Record<BatchStatus, string> = {
    analyzing: "Reading file",
    waiting: "Ready",
    processing: "Optimizing",
    completed: "Ready to send",
    noChange: "Already fits",
    failed: "Needs attention",
    cancelled: "Cancelled",
  };
  return labels[status];
}

export function pathKey(path: string): string {
  return path.replace(/\\/g, "/").toLocaleLowerCase();
}
