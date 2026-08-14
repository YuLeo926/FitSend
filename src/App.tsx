import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  ArrowDown,
  Check,
  CheckCircle2,
  ChevronRight,
  CircleStop,
  CircleAlert,
  Clock3,
  Clipboard,
  FileImage,
  FileVideo2,
  FolderOpen,
  Gauge,
  HardDrive,
  LoaderCircle,
  LockKeyhole,
  RotateCcw,
  Sparkles,
  UploadCloud,
} from "lucide-react";
import { formatBytes, formatDuration, formatElapsed, outputDefaultPath, savedPercent } from "./domain/format";
import { bytesFromCustomLimit, profileById, profiles } from "./domain/profiles";
import type { CompressionPlan, CompressionStrategy, MediaAnalysis, ProcessProgress, ProcessResult } from "./domain/types";
import "./styles.css";

type Phase = "idle" | "analyzing" | "ready" | "processing" | "success" | "error";

const fileFilters = [
  { name: "Images & videos", extensions: ["jpg", "jpeg", "png", "mp4", "mov", "mkv", "webm"] },
];

function App() {
  const [phase, setPhase] = useState<Phase>("idle");
  const [analysis, setAnalysis] = useState<MediaAnalysis | null>(null);
  const [plan, setPlan] = useState<CompressionPlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const [result, setResult] = useState<ProcessResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [profileId, setProfileId] = useState("discord");
  const [customValue, setCustomValue] = useState(10);
  const [customUnit, setCustomUnit] = useState<"KB" | "MB">("MB");
  const [strategy] = useState<CompressionStrategy>("balanced");
  const [dragActive, setDragActive] = useState(false);
  const [copied, setCopied] = useState(false);
  const [progress, setProgress] = useState<ProcessProgress | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [jobStartedAt, setJobStartedAt] = useState<number | null>(null);
  const [attemptStartedAt, setAttemptStartedAt] = useState<number | null>(null);
  const [clockNow, setClockNow] = useState(Date.now());
  const activeJobRef = useRef<string | null>(null);
  const activeAttemptRef = useRef(1);

  const activeProfile = profileById(profileId);
  const targetBytes = useMemo(
    () =>
      profileId === "custom"
        ? bytesFromCustomLimit(customValue, customUnit)
        : activeProfile.maxBytes,
    [activeProfile.maxBytes, customUnit, customValue, profileId],
  );

  const loadFile = useCallback(async (path: string) => {
    setPhase("analyzing");
    setAnalysis(null);
    setPlan(null);
    setPlanError(null);
    setResult(null);
    setError(null);

    try {
      const media = await invoke<MediaAnalysis>("analyze_media", { path });
      setAnalysis(media);
      setPhase("ready");
    } catch (reason) {
      setError(String(reason));
      setPhase("error");
    }
  }, []);

  const chooseFile = useCallback(async () => {
    if (!isTauri()) {
      setError("This is the browser preview. Open the FitSend desktop app to choose and process local files.");
      setPhase("error");
      return;
    }

    try {
      const selected = await open({ multiple: false, directory: false, filters: fileFilters });
      if (typeof selected === "string") await loadFile(selected);
    } catch (reason) {
      setError(`FitSend could not open the file picker: ${String(reason)}`);
      setPhase("error");
    }
  }, [loadFile]);

  useEffect(() => {
    if (!isTauri()) return;

    let disposed = false;
    let removeListener: (() => void) | undefined;

    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (disposed) return;
        if (event.payload.type === "enter" || event.payload.type === "over") setDragActive(true);
        if (event.payload.type === "leave") setDragActive(false);
        if (event.payload.type === "drop") {
          setDragActive(false);
          const firstPath = event.payload.paths[0];
          if (firstPath) void loadFile(firstPath);
        }
      })
      .then((unlisten) => {
        if (disposed) unlisten();
        else removeListener = unlisten;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      removeListener?.();
    };
  }, [loadFile]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let removeListener: (() => void) | undefined;

    listen<ProcessProgress>("fitsend://process-progress", (event) => {
      if (event.payload.jobId !== activeJobRef.current) return;
      if (event.payload.attempt !== activeAttemptRef.current) {
        activeAttemptRef.current = event.payload.attempt;
        setAttemptStartedAt(Date.now());
      }
      setProgress(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else removeListener = unlisten;
    }).catch(() => undefined);

    return () => {
      disposed = true;
      removeListener?.();
    };
  }, []);

  useEffect(() => {
    if (phase !== "processing") return;
    const timer = window.setInterval(() => setClockNow(Date.now()), 500);
    return () => window.clearInterval(timer);
  }, [phase]);

  useEffect(() => {
    if (!analysis || phase === "processing") return;
    let current = true;
    setPlan(null);
    setPlanError(null);

    if (targetBytes < 8 * 1024) {
      setPlanError(targetBytes <= 0 ? "Enter a size greater than zero." : "Choose a target of at least 8 KB.");
      return;
    }

    invoke<CompressionPlan>("build_plan", { request: { analysis, targetBytes, strategy } })
      .then((nextPlan) => {
        if (current) setPlan(nextPlan);
      })
      .catch((reason) => {
        if (!current) return;
        setPlanError(String(reason));
      });

    return () => {
      current = false;
    };
  }, [analysis, phase, strategy, targetBytes]);

  const makeItFit = async () => {
    if (!analysis || !plan || !plan.feasible || targetBytes <= 0) return;
    const outputPath = await save({
      defaultPath: outputDefaultPath(analysis.path, plan.outputExtension),
      filters: [{ name: plan.outputExtension.toUpperCase(), extensions: [plan.outputExtension] }],
    });
    if (!outputPath) return;

    const jobId = crypto.randomUUID();
    const startedAt = Date.now();
    activeJobRef.current = jobId;
    activeAttemptRef.current = 1;
    setJobStartedAt(startedAt);
    setAttemptStartedAt(startedAt);
    setClockNow(startedAt);
    setProgress({ jobId, percent: 0, stage: "Starting local processing", encodedSeconds: null, attempt: 1 });
    setCancelling(false);
    setPhase("processing");
    setError(null);
    setResult(null);
    try {
      const nextResult = await invoke<ProcessResult>("process_media", {
        jobId,
        request: { analysis, targetBytes, outputPath, strategy },
      });
      setResult(nextResult);
      setPhase("success");
    } catch (reason) {
      const message = String(reason);
      if (message.includes("PROCESS_CANCELLED")) {
        setPhase("ready");
      } else {
        setError(message);
        setPhase("error");
      }
    } finally {
      activeJobRef.current = null;
      setCancelling(false);
      setProgress(null);
    }
  };

  const cancelProcessing = async () => {
    const jobId = activeJobRef.current;
    if (!jobId || cancelling) return;
    setCancelling(true);
    setProgress((current) => current ? { ...current, stage: "Stopping safely…" } : current);
    try {
      await invoke<boolean>("cancel_process", { jobId });
    } catch (reason) {
      setError(String(reason));
      setPhase("error");
    }
  };

  const reset = () => {
    setPhase("idle");
    setAnalysis(null);
    setPlan(null);
    setPlanError(null);
    setResult(null);
    setError(null);
    setCopied(false);
    setProgress(null);
    setCancelling(false);
    setJobStartedAt(null);
    setAttemptStartedAt(null);
    activeJobRef.current = null;
  };

  const copyPath = async () => {
    if (!result) return;
    await navigator.clipboard.writeText(result.outputPath);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };

  const isWorking = phase === "analyzing" || phase === "processing";
  const elapsedMs = jobStartedAt === null ? 0 : Math.max(0, clockNow - jobStartedAt);
  const attemptElapsedMs = attemptStartedAt === null ? 0 : Math.max(0, clockNow - attemptStartedAt);
  const etaMs = progress && progress.percent >= 3 && progress.percent < 100
    ? Math.round(attemptElapsedMs * (100 - progress.percent) / progress.percent)
    : null;

  return (
    <main className="app-shell">
      <header className="topbar">
        <button className="brand" type="button" onClick={reset} aria-label="FitSend home">
          <span className="brand-mark" aria-hidden="true">
            <ArrowDown size={17} strokeWidth={2.6} />
          </span>
          <span>FitSend</span>
        </button>
        <div className="privacy-note"><LockKeyhole size={14} /> Files stay on this computer</div>
      </header>

      <section className="hero" aria-labelledby="page-title">
        <div className="eyebrow"><Sparkles size={14} /> Upload limits, handled</div>
        <h1 id="page-title">Make your file fit.<br />Keep it looking good.</h1>
        <p>Choose where it’s going. FitSend makes the best copy under the limit—and checks it before you send.</p>
      </section>

      <section className="workspace" aria-live="polite">
        <div className="main-column">
          {!analysis && phase !== "error" ? (
            <button
              className={`drop-zone ${dragActive ? "is-dragging" : ""}`}
              type="button"
              onClick={() => void chooseFile()}
              disabled={isWorking}
            >
              <span className="drop-icon"><UploadCloud size={31} /></span>
              {phase === "analyzing" ? (
                <>
                  <strong><LoaderCircle className="spin" size={19} /> Reading your file…</strong>
                  <span>Checking size, dimensions and format locally</span>
                </>
              ) : (
                <>
                  <strong>Drop an image or video here</strong>
                  <span>or click to choose · JPG, PNG, MP4, MOV, MKV, WebM</span>
                </>
              )}
            </button>
          ) : null}

          {analysis ? (
            <div className="file-card">
              <div className={`file-type-icon ${analysis.kind}`}>
                {analysis.kind === "image" ? <FileImage size={25} /> : <FileVideo2 size={25} />}
              </div>
              <div className="file-identity">
                <span className="section-label">Original file</span>
                <strong title={analysis.name}>{analysis.name}</strong>
                <div className="file-facts">
                  <span>{formatBytes(analysis.sizeBytes)}</span>
                  <i />
                  <span>{analysis.width} × {analysis.height}</span>
                  {analysis.durationSeconds !== null ? <><i /><span>{formatDuration(analysis.durationSeconds)}</span></> : null}
                  <i />
                  <span>{analysis.extension.toUpperCase()}</span>
                </div>
              </div>
              <button className="quiet-button" type="button" onClick={() => void chooseFile()} disabled={isWorking}>
                Replace
              </button>
            </div>
          ) : null}

          {analysis ? (
            <div className="plan-panel">
              <div className="plan-heading">
                <div>
                  <span className="section-label">Fit plan</span>
                  <h2>{plan?.summary ?? "Calculating the best fit…"}</h2>
                </div>
                {plan ? <span className={`quality-badge ${plan.feasible ? "good" : "warning"}`}>{plan.qualityLabel}</span> : null}
              </div>

              {plan ? (
                <>
                  <div className="size-route">
                    <div><span>Current</span><strong>{formatBytes(analysis.sizeBytes)}</strong></div>
                    <ChevronRight size={20} />
                    <div><span>Expected</span><strong>{formatBytes(plan.estimatedBytes)}</strong></div>
                    <ChevronRight size={20} />
                    <div className="route-target"><span>Must be under</span><strong>{formatBytes(plan.targetBytes)}</strong></div>
                  </div>

                  <div className="operation-line">
                    <Gauge size={17} />
                    <span>{plan.operation}</span>
                  </div>

                  {plan.warnings.length > 0 ? (
                    <div className="warnings">
                      {plan.warnings.map((warning) => <p key={warning}><CircleAlert size={16} /> {warning}</p>)}
                    </div>
                  ) : (
                    <p className="safe-note"><Check size={16} /> Output will be measured again before it is accepted.</p>
                  )}

                  {phase === "processing" && progress ? (
                    <div className="processing-panel">
                      <div className="progress-heading">
                        <div>
                          <span>{cancelling ? "Cancelling" : progress.stage}</span>
                          <strong>{progress.percent}%</strong>
                        </div>
                        <div className="progress-track" role="progressbar" aria-valuenow={progress.percent} aria-valuemin={0} aria-valuemax={100}>
                          <span style={{ width: `${progress.percent}%` }} />
                        </div>
                      </div>
                      <div className="progress-meta">
                        <span><Clock3 size={14} /> {formatElapsed(elapsedMs)} elapsed</span>
                        <span>{etaMs === null ? "Calculating time left…" : `About ${formatElapsed(etaMs)} left`}</span>
                        {progress.attempt > 1 ? <span>Attempt {progress.attempt} of 3</span> : null}
                      </div>
                      <button className="cancel-button" type="button" onClick={() => void cancelProcessing()} disabled={cancelling}>
                        {cancelling ? <LoaderCircle className="spin" size={17} /> : <CircleStop size={17} />}
                        {cancelling ? "Cleaning up…" : "Cancel"}
                      </button>
                    </div>
                  ) : (
                    <button
                      className="primary-button"
                      type="button"
                      onClick={() => void makeItFit()}
                      disabled={!plan.feasible || isWorking || targetBytes <= 0}
                    >
                      <Sparkles size={18} /> Make it fit
                    </button>
                  )}
                </>
              ) : planError ? (
                <div className="plan-error"><CircleAlert size={18} /><span>{planError}</span></div>
              ) : <div className="plan-skeleton"><span /><span /><span /></div>}
            </div>
          ) : null}

          {phase === "success" && result ? (
            <div className="success-card">
              <div className="success-check"><CheckCircle2 size={32} /></div>
              <div className="success-copy">
                <span className="section-label">Ready to send</span>
                <h2>{formatBytes(result.outputBytes)} — verified under {formatBytes(result.targetBytes)}</h2>
                <p title={result.outputPath}>{result.outputPath}</p>
                <div className="result-comparison">
                  <div><span>Original</span><strong>{formatBytes(analysis?.sizeBytes ?? 0)}</strong></div>
                  <div><span>Output</span><strong>{formatBytes(result.outputBytes)}</strong></div>
                  <div><span>Saved</span><strong>{savedPercent(analysis?.sizeBytes ?? 0, result.outputBytes)}%</strong></div>
                  <div><span>Dimensions</span><strong>{analysis?.width} × {analysis?.height} → {result.width} × {result.height}</strong></div>
                  <div><span>Processing</span><strong>{formatElapsed(result.durationMs)}</strong></div>
                  <div><span>Attempts</span><strong>{result.attempts}</strong></div>
                </div>
                <div className="success-actions">
                  <button className="primary-button compact" type="button" onClick={() => void revealItemInDir(result.outputPath)}>
                    <FolderOpen size={17} /> Show in folder
                  </button>
                  <button className="secondary-button" type="button" onClick={() => void copyPath()}>
                    {copied ? <Check size={17} /> : <Clipboard size={17} />} {copied ? "Copied" : "Copy path"}
                  </button>
                  <button className="quiet-button" type="button" onClick={reset}><RotateCcw size={16} /> Another file</button>
                </div>
              </div>
            </div>
          ) : null}

          {phase === "error" && error ? (
            <div className="error-card" role="alert">
              <CircleAlert size={24} />
              <div><strong>FitSend needs your attention</strong><p>{error}</p></div>
              <button className="secondary-button" type="button" onClick={reset}>Try another</button>
            </div>
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
                disabled={isWorking}
              >
                <span className={`profile-dot ${profile.accent}`} />
                <span><strong>{profile.name}</strong><small>{profile.description}</small></span>
                <span className="radio-check">{profileId === profile.id ? <Check size={13} /> : null}</span>
              </button>
            ))}
          </div>

          {profileId === "custom" ? (
            <div className="custom-limit">
              <label htmlFor="custom-size">Maximum file size</label>
              <div>
                <input
                  id="custom-size"
                  type="number"
                  min="0.01"
                  step="0.1"
                  value={customValue}
                  disabled={isWorking}
                  onChange={(event) => setCustomValue(Number(event.target.value))}
                />
                <select disabled={isWorking} value={customUnit} onChange={(event) => setCustomUnit(event.target.value as "KB" | "MB")}>
                  <option>MB</option><option>KB</option>
                </select>
              </div>
              {targetBytes <= 0 ? <p>Enter a size greater than zero.</p> : null}
            </div>
          ) : null}

          <div className="constraint-receipt">
            <div><span>Target ceiling</span><strong>{targetBytes > 0 ? formatBytes(targetBytes) : "—"}</strong></div>
            <div><span>Processing</span><strong>On device</strong></div>
            <div><span>Final check</span><strong>Required</strong></div>
          </div>

          <div className="local-promise"><HardDrive size={17} /><span><strong>Nothing is uploaded.</strong> Your original stays untouched.</span></div>
        </aside>
      </section>

      <footer><span>FitSend 0.1.3</span><span>Images + video · Video powered by FFmpeg</span></footer>
    </main>
  );
}

export default App;
