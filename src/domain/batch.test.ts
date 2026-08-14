import { describe, expect, it } from "vitest";
import { batchTotals, overallProgress, pathKey, primaryActionLabel, statusLabel, type BatchItem } from "./batch";
import type { MediaAnalysis, ProcessResult } from "./types";

function media(sizeBytes: number): MediaAnalysis {
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

function result(outputBytes: number, outcome: "created" | "noChange"): ProcessResult {
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

function item(overrides: Partial<BatchItem>): BatchItem {
  return {
    id: crypto.randomUUID(),
    path: "C:/photo.png",
    status: "waiting",
    analysis: media(1_000),
    plan: null,
    progress: null,
    result: null,
    error: null,
    ...overrides,
  };
}

describe("batch calculations", () => {
  it("counts mixed terminal states and bytes", () => {
    const totals = batchTotals([
      item({ status: "completed", analysis: media(1_000), result: result(600, "created") }),
      item({ status: "noChange", analysis: media(500), result: result(500, "noChange") }),
      item({ status: "failed", analysis: media(800), error: "broken" }),
    ]);
    expect(totals).toMatchObject({
      completed: 1,
      noChange: 1,
      failed: 1,
      originalBytes: 2_300,
      sendableBytes: 1_100,
      savedBytes: 400,
    });
  });

  it("includes the active item fraction in overall progress", () => {
    expect(overallProgress([
      item({ status: "completed" }),
      item({ status: "processing", progress: { jobId: "a", percent: 50, stage: "Encoding", encodedSeconds: null, attempt: 1 } }),
      item({ status: "waiting" }),
    ])).toBe(50);
  });

  it("normalizes Windows paths and supplies plain-language labels", () => {
    expect(pathKey("C:\\Photos\\ONE.PNG")).toBe("c:/photos/one.png");
    expect(primaryActionLabel(1)).toBe("Optimize 1 file");
    expect(primaryActionLabel(8)).toBe("Optimize 8 files");
    expect(statusLabel("noChange")).toBe("No change needed");
    expect(statusLabel("failed")).toBe("Needs attention");
  });
});
