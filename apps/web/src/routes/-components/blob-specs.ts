import type { BlobSpec } from "./blob-canvas";

export const PRIMARY_CLUSTER_SPECS: BlobSpec[] = [
  {
    alpha: 0.55,
    baseRadius: 0.42,
    colorVar: "--primary",
    cx: 0.46,
    cy: 0.44,
    driftAmp: 0.015,
    driftPhase: 0,
    driftSpeed: 0.12,
  },
  {
    alpha: 0.45,
    baseRadius: 0.36,
    colorVar: "--primary",
    cx: 0.6,
    cy: 0.56,
    driftAmp: 0.018,
    driftPhase: 1.9,
    driftSpeed: 0.1,
  },
  {
    alpha: 0.4,
    baseRadius: 0.32,
    colorVar: "--primary",
    cx: 0.4,
    cy: 0.6,
    driftAmp: 0.016,
    driftPhase: 3.6,
    driftSpeed: 0.14,
  },
  {
    alpha: 0.35,
    baseRadius: 0.28,
    colorVar: "--primary",
    cx: 0.56,
    cy: 0.36,
    driftAmp: 0.014,
    driftPhase: 5.2,
    driftSpeed: 0.11,
  },
];

export const PRIMARY_MELT_COLOR =
  "color-mix(in oklch, var(--primary) 45%, white 55%)";
