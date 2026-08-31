import { describe, expect, it } from "vitest";
import { allItemsTerminal, batchBadgeState, batchProof, batchTotals, cancelWaitingItems, failWaitingItems, isBatchAnalysisComplete, isBatchConfigurationLocked, overallProgress, pathKey, primaryActionLabel, statusLabel } from "./batch";
import { batchItem, completedItem, failedItem, media, noChangeItem, result } from "./batch.test-fixtures";
import { ruleById } from "./profiles";

describe("batch calculations", () => {
  it("counts mixed terminal states and bytes", () => {
    const totals = batchTotals([
      batchItem({ status: "completed", analysis: media(1_000), result: result(600, "created") }),
      batchItem({ status: "noChange", analysis: media(500), result: result(500, "noChange") }),
      batchItem({ status: "failed", analysis: media(800), error: "broken" }),
    ], ruleById("discord-safe"));
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

  it("proves every accepted per-file result without claiming failed rows", () => {
    const items = [completedItem("a", 900_000), failedItem("b", 2_000_000)];
    const proof = batchProof(items, { ...ruleById("discord-safe"), maxBytes: 1_000_000 });
    expect(proof.valid).toBe(true);
    expect(proof.allSelectedAccepted).toBe(false);
    expect(proof.headline).toBe("1 accepted file is under 977 KB; 1 file needs attention");
  });

  it("uses actual accepted totals for aggregate proof", () => {
    const items = [completedItem("a", 9_000_000), noChangeItem("b", 3_000_000)];
    const proof = batchProof(items, { ...ruleById("gmail-personal"), maxBytes: 24 * 1024 * 1024 });
    expect(proof.valid).toBe(true);
    expect(proof.acceptedBytes).toBe(12_000_000);
    expect(proof.headline).toContain("verified under 24 MB");
  });

  it("rejects an aggregate proof above the real ceiling", () => {
    const items = [completedItem("a", 13_000_000), completedItem("b", 13_000_000)];
    expect(batchProof(items, ruleById("gmail-personal")).valid).toBe(false);
  });

  it("exposes scope-aware accepted totals to the summary", () => {
    const totals = batchTotals(
      [completedItem("a", 9_000_000), failedItem("b", 2_000_000)],
      ruleById("gmail-personal"),
    );
    expect(totals).toMatchObject({
      acceptedFiles: 1,
      acceptedBytes: 9_000_000,
      limitScope: "batchTotal",
      workingCeilingBytes: 24 * 1024 * 1024,
      allSelectedAccepted: false,
      proofValid: true,
    });
  });

  it("uses completed output bytes, no-change source fallback, and zero failed or cancelled bytes", () => {
    const items = [
      completedItem("created", 8_000_000),
      batchItem({ id: "unchanged", status: "noChange", analysis: media(3_000_000), result: null }),
      failedItem("failed", 20_000_000),
      batchItem({ id: "cancelled", status: "cancelled", analysis: media(20_000_000) }),
    ];
    const proof = batchProof(items, ruleById("gmail-personal"));
    expect(proof).toMatchObject({
      acceptedFiles: 2,
      acceptedBytes: 11_000_000,
      attentionFiles: 2,
      valid: true,
      allSelectedAccepted: false,
      tone: "partial",
    });
  });

  it("emits complete and invalid per-file proof copy without overstating readiness", () => {
    const rule = { ...ruleById("discord-safe"), maxBytes: 1_000_000 };
    expect(batchProof([completedItem("a", 800_000), noChangeItem("b", 900_000)], rule)).toMatchObject({
      tone: "success",
      headline: "2 accepted files are each under 977 KB",
    });
    expect(batchProof([completedItem("a", 1_000_001)], rule)).toMatchObject({
      valid: false,
      allSelectedAccepted: true,
      tone: "invalid",
      headline: "1 accepted file could not be verified under 977 KB",
    });
  });

  it("emits partial and invalid aggregate proof copy from actual accepted totals", () => {
    const rule = ruleById("gmail-personal");
    expect(batchProof([completedItem("a", 9_000_000), failedItem("b", 30_000_000)], rule)).toMatchObject({
      tone: "partial",
      headline: "1 accepted file total 8.6 MB — verified under 24 MB; 1 file needs attention",
    });
    expect(batchProof([completedItem("a", 13_000_000), completedItem("b", 13_000_000)], rule)).toMatchObject({
      valid: false,
      allSelectedAccepted: true,
      tone: "invalid",
      headline: "2 accepted files total 25 MB — not verified under 24 MB",
    });
  });

  it("keeps totals and proof validity identical and preserves the sendable-bytes alias", () => {
    const cases = [
      { items: [completedItem("a", 800_000)], rule: ruleById("discord-safe") },
      { items: [completedItem("a", 20_000_000)], rule: ruleById("discord-safe") },
      { items: [completedItem("a", 9_000_000), failedItem("b", 2_000_000)], rule: ruleById("gmail-personal") },
      { items: [completedItem("a", 13_000_000), completedItem("b", 13_000_000)], rule: ruleById("gmail-personal") },
    ];
    for (const { items, rule } of cases) {
      const proof = batchProof(items, rule);
      const totals = batchTotals(items, rule);
      expect(totals.proofValid).toBe(proof.valid);
      expect(totals.acceptedBytes).toBe(proof.acceptedBytes);
      expect(totals.sendableBytes).toBe(totals.acceptedBytes);
    }
  });

  it("derives terminal badge copy and tone from proof state", () => {
    const rule = ruleById("gmail-personal");
    expect(batchBadgeState(batchProof([completedItem("a", 9_000_000)], rule), true, true, 0)).toEqual({
      text: "1 verified",
      tone: "good",
    });
    expect(batchBadgeState(batchProof([completedItem("a", 9_000_000), failedItem("b", 2_000_000)], rule), true, true, 0)).toEqual({
      text: "1 accepted",
      tone: "partial",
    });
    expect(batchBadgeState(batchProof([completedItem("a", 13_000_000), completedItem("b", 13_000_000)], rule), true, true, 0)).toEqual({
      text: "Verification failed",
      tone: "invalid",
    });
  });
});
