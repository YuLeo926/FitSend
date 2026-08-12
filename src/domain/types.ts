export type MediaKind = "image" | "video";

export type DestinationProfile = {
  id: string;
  name: string;
  shortLabel: string;
  description: string;
  maxBytes: number;
  acceptedKinds: MediaKind[];
  accent: "coral" | "blue" | "ink";
};

export type MediaAnalysis = {
  path: string;
  name: string;
  extension: string;
  kind: MediaKind;
  sizeBytes: number;
  width: number;
  height: number;
  durationSeconds: number | null;
  videoCodec: string | null;
  audioCodec: string | null;
  hasAudio: boolean;
  hasAlpha: boolean;
  ffmpegAvailable: boolean;
};

export type CompressionPlan = {
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
};

export type PlanRequest = {
  analysis: MediaAnalysis;
  targetBytes: number;
};

export type ProcessRequest = {
  analysis: MediaAnalysis;
  targetBytes: number;
  outputPath: string;
};
