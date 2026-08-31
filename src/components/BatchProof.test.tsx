import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { batchProof, batchTotals } from "../domain/batch";
import { batchItem, completedItem, failedItem } from "../domain/batch.test-fixtures";
import { ruleById } from "../domain/profiles";
import { BatchSummary } from "./BatchSummary";
import { FileQueue } from "./FileQueue";

describe("scope-correct batch proof presentation", () => {
  it("renders a partial receipt without claiming the whole selection is ready", () => {
    const items = [completedItem("accepted", 9_000_000), failedItem("failed", 2_000_000)];
    const rule = ruleById("gmail-personal");
    const html = renderToStaticMarkup(
      <BatchSummary totals={batchTotals(items, rule)} proof={batchProof(items, rule)} destination="Gmail" />,
    );
    expect(html).toContain('class="batch-summary partial"');
    expect(html).toContain("1 file needs attention");
    expect(html).not.toContain("ready for Gmail");
  });

  it("renders an invalid receipt with error styling and no ready claim", () => {
    const items = [completedItem("a", 13_000_000), completedItem("b", 13_000_000)];
    const rule = ruleById("gmail-personal");
    const html = renderToStaticMarkup(
      <BatchSummary totals={batchTotals(items, rule)} proof={batchProof(items, rule)} destination="Gmail" />,
    );
    expect(html).toContain('class="batch-summary invalid"');
    expect(html).toContain('class="summary-proof proof-invalid"');
    expect(html).toContain("Verification failed for Gmail");
    expect(html).not.toContain("ready for Gmail");
  });

  it("shows aggregate allocations only on waiting or processing rows", () => {
    const allocatedWaiting = { ...batchItem("waiting", 2_000_000), allocationBytes: 900_000 };
    const allocatedProcessing = { ...batchItem({ id: "processing", status: "processing" }), allocationBytes: 900_000 };
    const allocatedComplete = { ...completedItem("complete", 800_000), allocationBytes: 900_000 };
    const renderQueue = (limitScope: "perFile" | "batchTotal", items = [allocatedWaiting]) => renderToStaticMarkup(
      <FileQueue items={items} limitScope={limitScope} running={false} onRemove={vi.fn()} />,
    );
    expect(renderQueue("batchTotal")).toContain("Budget ≤ 879 KB");
    expect(renderQueue("batchTotal", [allocatedProcessing])).toContain("Budget ≤ 879 KB");
    expect(renderQueue("perFile")).not.toContain("Budget ≤");
    expect(renderQueue("batchTotal", [allocatedComplete])).not.toContain("Budget ≤");
  });
});
