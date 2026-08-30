import { describe, expect, it } from "vitest";
import { allItemsTerminal, batchTotals, cancelWaitingItems, failWaitingItems, isBatchAnalysisComplete, isBatchConfigurationLocked, overallProgress, pathKey, primaryActionLabel, statusLabel } from "./batch";
import { batchItem, media, result } from "./batch.test-fixtures";

describe("batch calculations", () => {
  it("counts mixed terminal states and bytes", () => {
    const totals = batchTotals([
      batchItem({ status: "completed", analysis: media(1_000), result: result(600, "created") }),
      batchItem({ status: "noChange", analysis: media(500), result: result(500, "noChange") }),
      batchItem({ status: "failed", analysis: media(800), error: "broken" }),
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
      batchItem({ status: "completed" }),
      batchItem({ status: "processing", progress: { jobId: "a", percent: 50, stage: "Encoding", encodedSeconds: null, attempt: 1 } }),
      batchItem({ status: "waiting" }),
    ])).toBe(50);
  });

  it("normalizes Windows paths and supplies plain-language labels", () => {
    expect(pathKey("C:\\Photos\\ONE.PNG")).toBe("c:/photos/one.png");
    expect(primaryActionLabel(1)).toBe("Make 1 file fit");
    expect(primaryActionLabel(8)).toBe("Make 8 files fit");
    expect(statusLabel("noChange")).toBe("Already fits");
    expect(statusLabel("failed")).toBe("Needs attention");
  });

  it("cancels only waiting rows and recognizes a mixed terminal batch", () => {
    const cancelled = cancelWaitingItems([
      batchItem({ status: "completed" }),
      batchItem({ status: "failed" }),
      batchItem({ status: "waiting" }),
      batchItem({ status: "waiting" }),
    ]);
    expect(cancelled.map((entry) => entry.status)).toEqual([
      "completed",
      "failed",
      "cancelled",
      "cancelled",
    ]);
    expect(allItemsTerminal(cancelled)).toBe(true);
  });

  it("locks the send plan after processing has produced a result", () => {
    expect(isBatchConfigurationLocked([batchItem({ status: "waiting" })])).toBe(false);
    expect(isBatchConfigurationLocked([
      batchItem({ status: "noChange", result: result(1_000, "noChange") }),
    ])).toBe(true);
  });

  it("locks configuration as soon as a budget allocation exists", () => {
    const item = { ...batchItem("a", 1_000_000), allocationBytes: 700_000 };
    expect(isBatchConfigurationLocked([item])).toBe(true);
  });

  it("marks every waiting row failed when the batch budget is infeasible", () => {
    const items = [batchItem("a", 1_000_000), batchItem("b", 1_000_000)];
    const next = failWaitingItems(items, "This total limit is too small for the selected file count.");
    expect(next.map((item) => item.status)).toEqual(["failed", "failed"]);
    expect(next.every((item) => item.error?.includes("too small"))).toBe(true);
  });

  it("does not allow a batch to start while any row is still analyzing", () => {
    expect(isBatchAnalysisComplete([
      batchItem({ status: "waiting" }),
      batchItem({ status: "analyzing", analysis: null }),
    ])).toBe(false);
  });

  it("allows analyzed waiting rows alongside failed terminal rows", () => {
    expect(isBatchAnalysisComplete([
      batchItem({ status: "waiting" }),
      batchItem({ status: "failed", error: "unreadable" }),
    ])).toBe(true);
  });
});
