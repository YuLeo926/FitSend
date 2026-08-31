import { Check, CircleAlert, Clipboard, FileImage, FileVideo2, FolderOpen, LoaderCircle, Trash2 } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { formatBytes, formatDuration, savedPercent } from "../domain/format";
import { allItemsTerminal, statusLabel, type BatchItem, type BatchProof } from "../domain/batch";
import type { LimitScope } from "../domain/types";

type Props = {
  items: BatchItem[];
  limitScope: LimitScope;
  proof: BatchProof | null;
  running: boolean;
  onRemove: (id: string) => void;
};

function displayName(item: BatchItem): string {
  return item.analysis?.name ?? item.path.replace(/\\/g, "/").split("/").pop() ?? item.path;
}

export function FileQueue({ items, limitScope, proof, running, onRemove }: Props) {
  const terminalProofInvalid = proof?.tone === "invalid" && allItemsTerminal(items);
  return (
    <section className="file-queue" aria-label="Selected files">
      {items.map((item, index) => {
        const analysis = item.analysis;
        const result = item.result;
        const isImage = analysis?.kind !== "video";
        const isNoChangeResult = item.status === "noChange" || result?.outcome === "noChange";
        const invalidAcceptedResult = terminalProofInvalid && (item.status === "completed" || item.status === "noChange");
        const presentationStatus = invalidAcceptedResult ? "failed" : item.status;
        const noChangeBytes = result && Number.isFinite(result.outputBytes) && result.outputBytes >= 0
          ? result.outputBytes
          : analysis?.sizeBytes;
        return (
          <article className={`queue-row status-${presentationStatus}${invalidAcceptedResult ? " proof-invalid-row" : ""}`} key={item.id}>
            <span className="queue-index">{String(index + 1).padStart(2, "0")}</span>
            <div className={`file-type-icon ${analysis?.kind ?? "image"}`}>
              {isImage ? <FileImage size={21} /> : <FileVideo2 size={21} />}
            </div>
            <div className="queue-main">
              <div className="queue-title-line">
                <strong title={item.path}>{displayName(item)}</strong>
                <span className={`queue-status ${presentationStatus}`}>
                  {item.status === "processing" || item.status === "analyzing" ? <LoaderCircle className="spin" size={13} /> : null}
                  {!invalidAcceptedResult && (item.status === "completed" || item.status === "noChange") ? <Check size={13} /> : null}
                  {item.status === "failed" || invalidAcceptedResult ? <CircleAlert size={13} /> : null}
                  {invalidAcceptedResult ? "Verification failed" : statusLabel(item.status)}
                </span>
              </div>
              {analysis ? (
                <div className="file-facts">
                  <span>{formatBytes(analysis.sizeBytes)}</span><i />
                  <span>{analysis.width} × {analysis.height}</span>
                  {analysis.durationSeconds !== null ? <><i /><span>{formatDuration(analysis.durationSeconds)}</span></> : null}
                  <i /><span>{analysis.extension.toUpperCase()}</span>
                  {limitScope === "batchTotal" && (item.status === "waiting" || item.status === "processing") && item.allocationBytes !== null ? (
                    <span className="allocation-label">Budget ≤ {formatBytes(item.allocationBytes)}</span>
                  ) : null}
                </div>
              ) : <span className="queue-path">{item.path}</span>}
              {item.status === "processing" && item.progress ? (
                <div className="row-progress">
                  <span style={{ width: `${item.progress.percent}%` }} />
                  <small>{item.progress.stage} · {item.progress.percent}%</small>
                </div>
              ) : null}
              {item.error ? <p className="row-error">{item.error}</p> : null}
              {result || isNoChangeResult ? (
                <div className="row-result">
                  <span>{isNoChangeResult ? `Verified ${formatBytes(noChangeBytes ?? -1)} — original kept` : `${formatBytes(result?.outputBytes ?? -1)} verified output`}</span>
                  {result?.outcome === "created" && analysis ? <span>{savedPercent(analysis.sizeBytes, result.outputBytes)}% smaller</span> : null}
                  {result ? <span>{result.reason}</span> : null}
                </div>
              ) : null}
            </div>
            <div className="queue-actions">
              {result ? (
                <>
                  <button className="icon-button" type="button" title="Copy result path" onClick={() => void navigator.clipboard.writeText(result.outputPath)}>
                    <Clipboard size={16} />
                  </button>
                  <button className="icon-button" type="button" title="Show in folder" onClick={() => void revealItemInDir(result.outputPath)}>
                    <FolderOpen size={17} />
                  </button>
                </>
              ) : null}
              {!running ? (
                <button className="icon-button remove" type="button" title="Remove file" onClick={() => onRemove(item.id)}>
                  <Trash2 size={16} />
                </button>
              ) : null}
            </div>
          </article>
        );
      })}
    </section>
  );
}
