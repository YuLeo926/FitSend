import { useCallback, useEffect, useMemo, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowDown,
  Check,
  CircleAlert,
  CircleStop,
  FilePlus2,
  HardDrive,
  LoaderCircle,
  LockKeyhole,
  ExternalLink,
  Sparkles,
  Trash2,
  UploadCloud,
} from "lucide-react";
import { BatchSummary } from "./components/BatchSummary";
import { CustomPlanEditor } from "./components/CustomPlanEditor";
import { DestinationPicker } from "./components/DestinationPicker";
import { FileQueue } from "./components/FileQueue";
import { StrategyPicker } from "./components/StrategyPicker";
import { batchBadgeState, isBatchConfigurationLocked, primaryActionLabel } from "./domain/batch";
import { formatBytes } from "./domain/format";
import {
  builtInRules,
  bytesFromCustomLimit,
  customRule,
  DISCORD_ATTACHMENTS_URL,
  DISCORD_CAPS_URL,
  GMAIL_ATTACHMENTS_URL,
  OUTLOOK_ATTACHMENTS_URL,
  ruleById,
  validateCustomLimit,
} from "./domain/profiles";
import { savedPlanToRule } from "./domain/savedPlans";
import { strategyById } from "./domain/strategies";
import type { CompressionStrategy, DestinationRule, LimitScope } from "./domain/types";
import { useBatchQueue } from "./hooks/useBatchQueue";
import { useSavedPlans } from "./hooks/useSavedPlans";
import "./styles.css";

const fileFilters = [
  { name: "Images & videos", extensions: ["jpg", "jpeg", "png", "mp4", "mov", "mkv", "webm"] },
];

const MIB = 1024 * 1024;
const officialSourceUrls = new Set([
  DISCORD_ATTACHMENTS_URL,
  DISCORD_CAPS_URL,
  GMAIL_ATTACHMENTS_URL,
  OUTLOOK_ATTACHMENTS_URL,
]);

const ruleDisplayNames: Record<string, string> = {
  "discord-safe": "Discord Free — Safe",
  "discord-basic": "Discord Nitro Basic",
  "discord-nitro": "Discord Nitro",
  "outlook-internet": "Outlook internet email",
};

function displayRuleName(rule: DestinationRule) {
  return ruleDisplayNames[rule.id] ?? rule.name;
}

function App() {
  const [ruleId, setRuleId] = useState("discord-safe");
  const [customValue, setCustomValue] = useState(10);
  const [customUnit, setCustomUnit] = useState<"KB" | "MB">("MB");
  const [customScope, setCustomScope] = useState<LimitScope>("perFile");
  const [customName, setCustomName] = useState("");
  const [strategy, setStrategy] = useState<CompressionStrategy>("balanced");
  const [dragActive, setDragActive] = useState(false);
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);

  const saved = useSavedPlans();
  const customBytes = useMemo(() => bytesFromCustomLimit(customValue, customUnit), [customUnit, customValue]);
  const customValidation = useMemo(() => validateCustomLimit(customBytes), [customBytes]);
  const savedRules = useMemo(() => saved.plans.map(savedPlanToRule), [saved.plans]);
  const activeRule = useMemo(
    () => ruleId === "custom"
      ? customRule(customBytes, customScope)
      : [...builtInRules, ...savedRules].find((rule) => rule.id === ruleId) ?? ruleById(ruleId),
    [customBytes, customScope, ruleId, savedRules],
  );
  const targetBytes = activeRule.maxBytes;
  const queue = useBatchQueue({ rule: activeRule, strategy });
  const selectedStrategy = strategyById(strategy);
  const readyCount = queue.items.filter((item) => item.status === "waiting").length;
  const analyzing = queue.items.some((item) => item.status === "analyzing");
  const hasFiles = queue.items.length > 0;
  const validTarget = activeRule.id !== "custom" || customValidation.valid;
  const configurationLocked = isBatchConfigurationLocked(queue.items);
  const configurationDisabled = queue.running || configurationLocked;
  const batchBadge = batchBadgeState(queue.proof, validTarget, queue.allTerminal, readyCount);

  const handleRuleChange = useCallback((rule: DestinationRule) => {
    if (configurationDisabled) return;
    saved.clearError();
    setRuleId(rule.id);
    if (rule.family === "saved") {
      const unit = rule.maxBytes >= MIB ? "MB" : "KB";
      setCustomValue(rule.maxBytes / (unit === "MB" ? MIB : 1024));
      setCustomUnit(unit);
      setCustomScope(rule.scope);
      setCustomName(rule.name);
    }
  }, [configurationDisabled, saved.clearError]);

  const openCustomEditor = useCallback(() => {
    if (configurationDisabled) return;
    saved.clearError();
    setRuleId("custom");
  }, [configurationDisabled, saved.clearError]);

  const updateCustomValue = useCallback((value: number) => {
    if (configurationDisabled) return;
    saved.clearError();
    setRuleId("custom");
    setCustomValue(value);
  }, [configurationDisabled, saved.clearError]);

  const updateCustomUnit = useCallback((unit: "KB" | "MB") => {
    if (configurationDisabled) return;
    saved.clearError();
    setRuleId("custom");
    setCustomUnit(unit);
  }, [configurationDisabled, saved.clearError]);

  const updateCustomScope = useCallback((scope: LimitScope) => {
    if (configurationDisabled) return;
    saved.clearError();
    setRuleId("custom");
    setCustomScope(scope);
  }, [configurationDisabled, saved.clearError]);

  const deleteActiveSavedPlan = useCallback(() => {
    if (configurationDisabled || activeRule.family !== "saved") return;
    if (saved.remove(activeRule.id)) setRuleId("discord-safe");
  }, [activeRule.family, activeRule.id, configurationDisabled, saved.remove]);

  const openRuleSource = useCallback(async () => {
    if (!activeRule.builtIn || !activeRule.sourceUrl || !officialSourceUrls.has(activeRule.sourceUrl)) return;
    try {
      await openUrl(activeRule.sourceUrl);
      setSourceError(null);
    } catch (reason) {
      setSourceError(`FitSend could not open the official source: ${String(reason)}`);
    }
  }, [activeRule.builtIn, activeRule.sourceUrl]);

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
              <FileQueue items={queue.items} limitScope={activeRule.scope} proof={queue.proof} running={queue.running} onRemove={queue.removeItem} />
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
                  <h2>{displayRuleName(activeRule)} · {selectedStrategy.name}</h2>
                </div>
                <span className={`quality-badge ${batchBadge.tone}`}>{batchBadge.text}</span>
              </div>
              <div className="batch-route">
                <div><span>Destination</span><strong>{activeRule.shortLabel}</strong></div>
                <div>
                  <span>{activeRule.scope === "perFile" ? "Limit for each file" : "Limit for all files"}</span>
                  <strong>{validTarget ? formatBytes(targetBytes) : "—"}</strong>
                </div>
                <div><span>Quality rule</span><strong>{selectedStrategy.name}</strong></div>
              </div>
              <p className="safe-note"><Check size={16} /> {selectedStrategy.description}. {activeRule.scope === "perFile"
                ? "FitSend checks each accepted file against this limit."
                : "FitSend checks the total of all accepted files against this limit."}</p>

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
                <div className={`batch-finished-note ${queue.proof.tone === "success" ? "" : queue.proof.tone}`.trim()}>
                  {queue.proof.tone === "invalid" ? <CircleAlert size={17} /> : <Check size={17} />}
                  {queue.proof.headline}.
                </div>
              )}
            </section>
          ) : null}

          {queue.allTerminal ? (
            <BatchSummary
              totals={queue.totals}
              proof={queue.proof}
              destination={activeRule.id === "custom" ? "your custom limit" : activeRule.shortLabel}
            />
          ) : null}
        </div>

        <aside className="destination-card">
          <div className="aside-heading">
            <span className="step-number">1</span>
            <div><span className="section-label">Destination</span><h2>Where is it going?</h2></div>
          </div>
          <DestinationPicker
            value={activeRule}
            savedRules={savedRules}
            disabled={configurationDisabled}
            onChange={handleRuleChange}
            onOpenCustom={openCustomEditor}
          />
          {saved.error && activeRule.family !== "custom" && activeRule.family !== "saved" ? <p role="alert">{saved.error}</p> : null}

          {activeRule.family === "custom" || activeRule.family === "saved" ? (
            <CustomPlanEditor
              value={customValue}
              unit={customUnit}
              scope={customScope}
              name={customName}
              validation={customValidation}
              disabled={configurationDisabled}
              savedPlanError={saved.error}
              canDelete={activeRule.family === "saved"}
              onValueChange={updateCustomValue}
              onUnitChange={updateCustomUnit}
              onScopeChange={updateCustomScope}
              onNameChange={(name) => {
                if (configurationDisabled) return;
                saved.clearError();
                setRuleId("custom");
                setCustomName(name);
              }}
              onSave={() => { void saved.save(customName, customScope, customBytes); }}
              onReplace={() => { void saved.replace(customName, customScope, customBytes); }}
              onDelete={deleteActiveSavedPlan}
            />
          ) : null}

          <StrategyPicker value={strategy} disabled={configurationDisabled} onChange={setStrategy} />

          {configurationLocked ? (
            <p className="config-lock-note">This destination and quality rule are locked to the verified results. Clear the batch to choose a new send plan.</p>
          ) : null}

          <div className="constraint-receipt">
            <div><span>Scope</span><strong>{activeRule.scope === "perFile" ? "Each file" : "All files together"}</strong></div>
            <div><span>Working ceiling</span><strong>{validTarget ? formatBytes(targetBytes) : "—"}</strong></div>
            <div><span>Published context</span><strong>{activeRule.publishedLimitLabel}</strong></div>
          </div>
          <p className="rule-note">{activeRule.ruleNote}</p>
          <div className="rule-source">
            <div>
              <span className="section-label">Rule source</span>
              <strong>{activeRule.sourceLabel}</strong>
              {activeRule.verifiedOn === "2026-08-30" ? <small>Checked 30 Aug 2026</small> : null}
            </div>
            {activeRule.builtIn && activeRule.sourceUrl && officialSourceUrls.has(activeRule.sourceUrl) ? (
              <button className="rule-source-link" type="button" onClick={() => void openRuleSource()} aria-label={`Open official source: ${activeRule.sourceLabel}`}>
                Source <ExternalLink size={13} />
              </button>
            ) : null}
          </div>
          {sourceError ? <p role="alert">{sourceError}</p> : null}
          <div className="local-promise"><HardDrive size={17} /><span><strong>Nothing is uploaded.</strong> Originals stay untouched.</span></div>
        </aside>
      </section>

      <footer><span>FitSend 0.3.0</span><span>Fits the limit · Protects quality · Verifies locally</span></footer>
    </main>
  );
}

export default App;
