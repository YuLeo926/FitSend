import type { BatchItem } from "./batch";
import type { MediaAnalysis, ProcessResult } from "./types";

export function media(sizeBytes: number): MediaAnalysis {
  return {
    path: "C:/photo.png",
    name: "photo.png",
    extension: "png",
    kind: "image",
    sizeBytes,
    width: 100,
    height: 100,
    durationSeconds: null,
    frameRate: null,
    rotationDegrees: 0,
    videoCodec: null,
    audioCodec: null,
    hasAudio: false,
    hasAlpha: false,
    ffmpegAvailable: true,
  };
}

export function result(outputBytes: number, outcome: "created" | "noChange"): ProcessResult {
  return {
    outputPath: "C:/photo.fitsend.jpg",
    outputBytes,
    targetBytes: 2_000,
    verified: true,
    attempts: outcome === "created" ? 1 : 0,
    width: 100,
    height: 100,
    durationMs: 100,
    outcome,
    reason: "verified",
    qualityScore: 0.99,
  };
}

export function batchItem(
  idOrOverrides: string | Partial<BatchItem> = crypto.randomUUID(),
  sourceBytes = 1_000,
): BatchItem {
  const overrides = typeof idOrOverrides === "string" ? {} : idOrOverrides;
  const id = typeof idOrOverrides === "string" ? idOrOverrides : crypto.randomUUID();
  return {
    id,
    path: "C:/photo.png",
    status: "waiting",
    analysis: media(sourceBytes),
    plan: null,
    progress: null,
    result: null,
    error: null,
    allocationBytes: null,
    ...overrides,
  };
}
