import { CheckCircle2, CircleAlert } from "lucide-react";
import type { BatchProof, BatchTotals } from "../domain/batch";
import { formatBytes } from "../domain/format";

type Props = {
  totals: BatchTotals;
  proof: BatchProof;
  destination: string;
};

export function BatchSummary({ totals, proof, destination }: Props) {
  const title = proof.tone === "success"
    ? `${proof.acceptedFiles} ${proof.acceptedFiles === 1 ? "file" : "files"} ready for ${destination}`
    : proof.tone === "partial"
      ? `${proof.acceptedFiles} accepted ${proof.acceptedFiles === 1 ? "file" : "files"} for ${destination}`
      : `Verification failed for ${destination}`;
  return (
    <section className={`batch-summary ${proof.tone === "success" ? "" : proof.tone}`.trim()}>
      <span className={`summary-mark ${proof.tone === "success" ? "" : proof.tone}`.trim()}>
        {proof.tone === "invalid" ? <CircleAlert size={26} /> : <CheckCircle2 size={26} />}
      </span>
      <div>
        <span className="section-label">{proof.tone === "invalid" ? "Verification issue" : "Measured and verified"}</span>
        <h2>{title}</h2>
        <p className={`summary-proof ${proof.tone === "invalid" ? "proof-invalid" : ""}`.trim()}>{proof.headline}. {proof.detail}</p>
        <div className="summary-metrics">
          <span><small>Original total</small><strong>{formatBytes(totals.originalBytes)}</strong></span>
          <span><small>Accepted total</small><strong>{formatBytes(totals.acceptedBytes)}</strong></span>
          <span><small>Saved</small><strong>{formatBytes(totals.savedBytes)}</strong></span>
          <span><small>Results</small><strong>{totals.completed} new · {totals.noChange} kept · {totals.failed} failed · {totals.cancelled} cancelled</strong></span>
        </div>
      </div>
    </section>
  );
}
