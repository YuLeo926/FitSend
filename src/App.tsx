import { useCallback, useEffect, useMemo, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowDown,
  Check,
  CircleAlert,
  CircleStop,
  FilePlus2,
  HardDrive,
  LoaderCircle,
  LockKeyhole,
  Sparkles,
  Trash2,
  UploadCloud,
} from "lucide-react";
import { BatchSummary } from "./components/BatchSummary";
import { FileQueue } from "./components/FileQueue";
import { StrategyPicker } from "./components/StrategyPicker";
import { isBatchConfigurationLocked, primaryActionLabel } from "./domain/batch";
import { formatBytes } from "./domain/format";
import { bytesFromCustomLimit, profileById, profiles } from "./domain/profiles";
import { strategyById } from "./domain/strategies";
import type { CompressionStrategy } from "./domain/types";
import { useBatchQueue } from "./hooks/useBatchQueue";
import "./styles.css";

const fileFilters = [
  { name: "Images & videos", extensions: ["jpg", "jpeg", "png", "mp4", "mov", "mkv", "webm"] },
];

function App() {
  const [profileId, setProfileId] = useState("discord");
  const [customValue, setCustomValue] = useState(10);
  const [customUnit, setCustomUnit] = useState<"KB" | "MB">("MB");
  const [strategy, setStrategy] = useState<CompressionStrategy>("balanced");
  const [dragActive, setDragActive] = useState(false);
  const [pickerError, setPickerError] = useState<string | null>(null);

  const activeProfile = profileById(profileId);
  const targetBytes = useMemo(
    () => profileId === "custom"
      ? bytesFromCustomLimit(customValue, customUnit)
      : activeProfile.maxBytes,
    [activeProfile.maxBytes, customUnit, customValue, profileId],
  );
  const queue = useBatchQueue({ targetBytes, strategy });
  const selectedStrategy = strategyById(strategy);
  const readyCount = queue.items.filter((item) => item.status === "waiting").length;
  const analyzing = queue.items.some((item) => item.status === "analyzing");
  const hasFiles = queue.items.length > 0;
  const validTarget = targetBytes >= 8 * 1024;
  const configurationLocked = isBatchConfigurationLocked(queue.items);
  const configurationDisabled = queue.running || configurationLocked;
  const batchBadge = !validTarget
    ? "Check limit"
    : queue.allTerminal
      ? `${queue.totals.completed + queue.totals.noChange} verified`
      : `${readyCount} to fit`;

  const addFiles = useCallback(async () => {
    if (!isTauri()) {
      setPickerError("This is the browser preview. Open the FitSend desktop app to choose and process local files.");
      return;
    }
    try {
      const selected = await open({ multiple: true, directory: false, filters: fileFilters });
      const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
      setPickerError(null);
      await queue.addPaths(paths);
    } catch (reason) {
      setPickerError(`FitSend could not open the file picker: ${String(reason)}`);
    }
  }, [queue.addPaths]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let removeListener: (() => void) | undefined;
    getCurrentWebview().onDragDropEvent((event) => {
      if (disposed || queue.running) return;
      if (event.payload.type === "enter" || event.payload.type === "over") setDragActive(true);
      if (event.payload.type === "leave") setDragActive(false);
      if (event.payload.type === "drop") {
        setDragActive(false);
        void queue.addPaths(event.payload.paths);
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else removeListener = unlisten;
    }).catch(() => undefined);
    return () => {
      disposed = true;
      removeListener?.();
    };
  }, [queue.addPaths, queue.running]);

  return (
    <main className="app-shell">
      <header className="topbar">
        <button className="brand" type="button" onClick={queue.clear} aria-label="FitSend home">
          <span className="brand-mark" aria-hidden="true"><ArrowDown size={17} strokeWidth={2.6} /></span>
          <span>FitSend</span>
        </button>
        <div className="privacy-note"><LockKeyhole size={14} /> Files stay on this computer</div>
      </header>

      <section className="hero" aria-labelledby="page-title">
        <div className="eyebrow"><Sparkles size={14} /> Built for the limit. Checked before you send.</div>
        <h1 id="page-title">Make every file<br />ready to send.</h1>
        <p>Choose where it’s going. FitSend makes a verified copy under the limit without crossing a conservative quality floor.</p>
      </section>

      <section className="workspace" aria-live="polite">
        <div className="main-column">
          {!hasFiles ? (
            <button
              className={`drop-zone ${dragActive ? "is-dragging" : ""}`}
              type="button"
              onClick={() => void addFiles()}
              disabled={queue.running}
            >
              <span className="drop-icon"><UploadCloud size={31} /></span>
              <strong>Drop files here to make them fit</strong>
              <span>or choose several images and videos · JPG, PNG, MP4, MOV, MKV, WebM</span>
            </button>
          ) : (
            <div className="queue-shell">
              <div className="queue-header">
                <div>
                  <span className="section-label">Selected batch</span>
                  <h2>{queue.items.length} {queue.items.length === 1 ? "file" : "files"} · {formatBytes(queue.totals.originalBytes)}</h2>
                </div>
                <div className="queue-header-actions">
                  <button className="secondary-button" type="button" onClick={() => void addFiles()} disabled={queue.running}>
                    <FilePlus2 size={16} /> Add files
                  </button>
                  <button className="icon-button remove" type="button" title="Clear list" onClick={queue.clear} disabled={queue.running}>
                    <Trash2 size={16} />
                  </button>
                </div>
              </div>
              <FileQueue items={queue.items} running={queue.running} onRemove={queue.removeItem} />
            </div>
          )}

          {pickerError ? (
            <div className="error-card" role="alert">
              <CircleAlert size={22} />
              <div><strong>FitSend needs your attention</strong><p>{pickerError}</p></div>
            </div>
          ) : null}

          {hasFiles ? (
            <section className="plan-panel batch-plan">
              <div className="plan-heading">
                <div>
                  <span className="section-label">Send plan</span>
                  <h2>{activeProfile.name} · {selectedStrategy.name}</h2>
                </div>
                <span className={`quality-badge ${validTarget ? "good" : "warning"}`}>{batchBadge}</span>
              </div>
              <div className="batch-route">
                <div><span>Destination</span><strong>{activeProfile.shortLabel}</strong></div>
                <div><span>Limit per file</span><strong>{validTarget ? formatBytes(targetBytes) : "—"}</strong></div>
                <div><span>Quality rule</span><strong>{selectedStrategy.name}</strong></div>
              </div>
              <p className="safe-note"><Check size={16} /> {selectedStrategy.description}. FitSend measures every accepted file again before marking it ready to send.</p>

              {queue.running ? (
                <div className="processing-panel batch-processing">
                  <div className="progress-heading">
                    <div><span>Making files fit, one at a time</span><strong>{queue.progress}%</strong></div>
                    <div className="progress-track" role="progressbar" aria-valuenow={queue.progress} aria-valuemin={0} aria-valuemax={100}>
                      <span style={{ width: `${queue.progress}%` }} />
                    </div>
                  </div>
                  <button className="cancel-button" type="button" onClick={() => void queue.cancel()}>
                    <CircleStop size={17} /> Cancel batch
                  </button>
                </div>
              ) : queue.hasRunnable || analyzing ? (
                <button
                  className="primary-button"
                  type="button"
                  disabled={!queue.hasRunnable || analyzing || !validTarget}
                  onClick={() => void queue.start()}
                >
                  {analyzing ? <LoaderCircle className="spin" size={18} /> : <Sparkles size={18} />}
                  {analyzing ? "Reading selected files…" : primaryActionLabel(readyCount)}
                </button>
              ) : (
                <div className="batch-finished-note"><Check size={17} /> Every sendable file in this batch has been verified.</div>
              )}
            </section>
          ) : null}

          {queue.allTerminal ? (
            <BatchSummary
              totals={queue.totals}
              destination={activeProfile.id === "custom" ? "your custom limit" : activeProfile.shortLabel}
              targetBytes={targetBytes}
            />
          ) : null}
        </div>

        <aside className="destination-card">
          <div className="aside-heading">
            <span className="step-number">1</span>
            <div><span className="section-label">Destination</span><h2>Where is it going?</h2></div>
          </div>
          <div className="profiles" role="radiogroup" aria-label="File destination">
            {profiles.map((profile) => (
              <button
                className={`profile-option ${profileId === profile.id ? "selected" : ""}`}
                type="button"
                role="radio"
                aria-checked={profileId === profile.id}
                key={profile.id}
                onClick={() => setProfileId(profile.id)}
                disabled={configurationDisabled}
              >
                <span className={`profile-dot ${profile.accent}`} />
                <span><strong>{profile.name}</strong><small>{profile.description}</small></span>
                <span className="radio-check">{profileId === profile.id ? <Check size={13} /> : null}</span>
              </button>
            ))}
          </div>

          {profileId === "custom" ? (
            <div className="custom-limit">
              <label htmlFor="custom-size">Maximum size for each file</label>
              <div>
                <input id="custom-size" type="number" min="0.01" step="0.1" value={customValue} disabled={configurationDisabled} onChange={(event) => setCustomValue(Number(event.target.value))} />
                <select disabled={configurationDisabled} value={customUnit} onChange={(event) => setCustomUnit(event.target.value as "KB" | "MB")}>
                  <option>MB</option><option>KB</option>
                </select>
              </div>
              {!validTarget ? <p>Choose at least 8 KB.</p> : null}
            </div>
          ) : null}

          <StrategyPicker value={strategy} disabled={configurationDisabled} onChange={setStrategy} />

          {configurationLocked ? (
            <p className="config-lock-note">This destination and quality rule are locked to the verified results. Clear the batch to choose a new send plan.</p>
          ) : null}

          <div className="constraint-receipt">
            <div><span>Destination</span><strong>{activeProfile.shortLabel}</strong></div>
            <div><span>Per-file limit</span><strong>{validTarget ? formatBytes(targetBytes) : "—"}</strong></div>
            <div><span>Proof before send</span><strong>Size verified</strong></div>
          </div>
          <div className="local-promise"><HardDrive size={17} /><span><strong>Nothing is uploaded.</strong> Originals stay untouched.</span></div>
        </aside>
      </section>

      <footer><span>FitSend 0.2.1</span><span>Fits the limit · Protects quality · Verifies locally</span></footer>
    </main>
  );
}

export default App;
