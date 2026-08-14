import { CheckCircle2 } from "lucide-react";
import type { BatchTotals } from "../domain/batch";
import { formatBytes } from "../domain/format";

export function BatchSummary({ totals }: { totals: BatchTotals }) {
  return (
    <section className="batch-summary">
      <span className="summary-mark"><CheckCircle2 size={26} /></span>
      <div>
        <span className="section-label">Batch complete</span>
        <h2>{totals.completed + totals.noChange} files ready to send</h2>
        <div className="summary-metrics">
          <span><small>Original total</small><strong>{formatBytes(totals.originalBytes)}</strong></span>
          <span><small>Sendable total</small><strong>{formatBytes(totals.sendableBytes)}</strong></span>
          <span><small>Saved</small><strong>{formatBytes(totals.savedBytes)}</strong></span>
          <span><small>Results</small><strong>{totals.completed} new · {totals.noChange} kept · {totals.failed} failed</strong></span>
        </div>
      </div>
    </section>
  );
}
