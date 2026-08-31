import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { batchProof, batchTotals } from "../domain/batch";
import { batchItem, completedItem, failedItem, media, result } from "../domain/batch.test-fixtures";
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
      <FileQueue items={items} limitScope={limitScope} proof={null} running={false} onRemove={vi.fn()} />,
    );
    expect(renderQueue("batchTotal")).toContain("Budget ≤ 879 KB");
    expect(renderQueue("batchTotal", [allocatedProcessing])).toContain("Budget ≤ 879 KB");
    expect(renderQueue("perFile")).not.toContain("Budget ≤");
    expect(renderQueue("batchTotal", [allocatedComplete])).not.toContain("Budget ≤");
  });

  it("replaces affirmative terminal row labels when the batch proof is invalid", () => {
    const items = [completedItem("a", 13_000_000), completedItem("b", 13_000_000)];
    const proof = batchProof(items, ruleById("gmail-personal"));
    const html = renderToStaticMarkup(
      <FileQueue items={items} limitScope="batchTotal" proof={proof} running={false} onRemove={vi.fn()} />,
    );
    expect(html).toContain("Verification failed");
    expect(html).toContain('class="queue-status failed"');
    expect(html).not.toContain("Ready to send");
    expect(html).not.toContain('class="queue-status completed"');
  });

  it("shows measured no-change result bytes even when source analysis differs", () => {
    const item = batchItem({
      id: "unchanged",
      status: "noChange",
      analysis: media(5_000_000),
      result: result(3_000_000, "noChange"),
    });
    const html = renderToStaticMarkup(
      <FileQueue items={[item]} limitScope="perFile" proof={null} running={false} onRemove={vi.fn()} />,
    );
    expect(html).toContain("Verified 2.9 MB — original kept");
    const fallbackHtml = renderToStaticMarkup(
      <FileQueue
        items={[batchItem({ id: "legacy-unchanged", status: "noChange", analysis: media(5_000_000), result: null })]}
        limitScope="perFile"
        proof={null}
        running={false}
        onRemove={vi.fn()}
      />,
    );
    expect(fallbackHtml).toContain("Verified 4.8 MB — original kept");
  });
});
