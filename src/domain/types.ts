export type MediaKind = "image" | "video";
export type CompressionStrategy = "precise" | "balanced" | "smallest";
export type ProcessOutcome = "created" | "noChange";

export type LimitScope = "perFile" | "batchTotal";
export type DestinationFamily = "discord" | "email" | "web" | "custom" | "saved";

export type DestinationRule = {
  id: string;
  family: DestinationFamily;
  name: string;
  shortLabel: string;
  description: string;
  scope: LimitScope;
  publishedLimitLabel: string;
  maxBytes: number;
  acceptedKinds: MediaKind[];
  sourceLabel: string;
  sourceUrl: string | null;
  verifiedOn: string | null;
  ruleNote: string;
  accent: "coral" | "blue" | "ink";
  builtIn: boolean;
};

// Compatibility bridge used by App.tsx until Task 6 migrates the UI.
export type DestinationProfile = DestinationRule;

export type MediaAnalysis = {
  path: string;
  name: string;
  extension: string;
  kind: MediaKind;
  sizeBytes: number;
  width: number;
  height: number;
  durationSeconds: number | null;
  frameRate: number | null;
  rotationDegrees: number;
  videoCodec: string | null;
  audioCodec: string | null;
  hasAudio: boolean;
  hasAlpha: boolean;
  ffmpegAvailable: boolean;
};

export type CompressionPlan = {
  strategy: CompressionStrategy;
  alreadyFits: boolean;
  feasible: boolean;
  targetBytes: number;
  estimatedBytes: number;
  operation: string;
  summary: string;
  qualityLabel: string;
  warnings: string[];
  outputExtension: string;
  videoBitrateKbps: number | null;
  audioBitrateKbps: number | null;
  width: number;
  height: number;
};

export type ProcessResult = {
  outputPath: string;
  outputBytes: number;
  targetBytes: number;
  verified: boolean;
  attempts: number;
  width: number;
  height: number;
  durationMs: number;
  outcome: ProcessOutcome;
  reason: string;
  qualityScore: number | null;
};

export type ProcessProgress = {
  jobId: string;
  percent: number;
  stage: string;
  encodedSeconds: number | null;
  attempt: number;
};

export type PlanRequest = {
  analysis: MediaAnalysis;
  targetBytes: number;
  strategy: CompressionStrategy;
};

export type ProcessRequest = {
  analysis: MediaAnalysis;
  targetBytes: number;
  outputPath: string;
  strategy: CompressionStrategy;
};
