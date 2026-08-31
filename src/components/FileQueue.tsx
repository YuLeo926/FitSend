import { Check, CircleAlert, Clipboard, FileImage, FileVideo2, FolderOpen, LoaderCircle, Trash2 } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { formatBytes, formatDuration, savedPercent } from "../domain/format";
import { statusLabel, type BatchItem } from "../domain/batch";
import type { LimitScope } from "../domain/types";

type Props = {
  items: BatchItem[];
  limitScope: LimitScope;
  running: boolean;
  onRemove: (id: string) => void;
};

function displayName(item: BatchItem): string {
  return item.analysis?.name ?? item.path.replace(/\\/g, "/").split("/").pop() ?? item.path;
}

export function FileQueue({ items, limitScope, running, onRemove }: Props) {
  return (
    <section className="file-queue" aria-label="Selected files">
      {items.map((item, index) => {
        const analysis = item.analysis;
        const result = item.result;
        const isImage = analysis?.kind !== "video";
        return (
          <article className={`queue-row status-${item.status}`} key={item.id}>
            <span className="queue-index">{String(index + 1).padStart(2, "0")}</span>
            <div className={`file-type-icon ${analysis?.kind ?? "image"}`}>
              {isImage ? <FileImage size={21} /> : <FileVideo2 size={21} />}
            </div>
            <div className="queue-main">
              <div className="queue-title-line">
                <strong title={item.path}>{displayName(item)}</strong>
                <span className={`queue-status ${item.status}`}>
                  {item.status === "processing" || item.status === "analyzing" ? <LoaderCircle className="spin" size={13} /> : null}
                  {item.status === "completed" || item.status === "noChange" ? <Check size={13} /> : null}
                  {item.status === "failed" ? <CircleAlert size={13} /> : null}
                  {statusLabel(item.status)}
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
              {result ? (
                <div className="row-result">
                  <span>{result.outcome === "noChange" ? "Already fits — original kept" : `${formatBytes(result.outputBytes)} verified output`}</span>
                  {result.outcome === "created" && analysis ? <span>{savedPercent(analysis.sizeBytes, result.outputBytes)}% smaller</span> : null}
                  <span>{result.reason}</span>
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
