import { CheckCircle2 } from "lucide-react";
import type { BatchTotals } from "../domain/batch";
import { formatBytes } from "../domain/format";

type Props = {
  totals: BatchTotals;
  destination: string;
  targetBytes: number;
};

export function BatchSummary({ totals, destination, targetBytes }: Props) {
  const verifiedFiles = totals.completed + totals.noChange;
  return (
    <section className="batch-summary">
      <span className="summary-mark"><CheckCircle2 size={26} /></span>
      <div>
        <span className="section-label">Measured and verified</span>
        <h2>{verifiedFiles} {verifiedFiles === 1 ? "file" : "files"} ready for {destination}</h2>
        <p className="summary-proof">Each accepted file is under {formatBytes(targetBytes)}. Originals remain untouched.</p>
        <div className="summary-metrics">
          <span><small>Original total</small><strong>{formatBytes(totals.originalBytes)}</strong></span>
          <span><small>Verified total</small><strong>{formatBytes(totals.sendableBytes)}</strong></span>
          <span><small>Saved</small><strong>{formatBytes(totals.savedBytes)}</strong></span>
          <span><small>Results</small><strong>{totals.completed} new · {totals.noChange} kept · {totals.failed} failed</strong></span>
        </div>
      </div>
    </section>
  );
}
