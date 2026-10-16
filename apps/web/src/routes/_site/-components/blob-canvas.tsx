import { useEffect, useRef } from "react";

const POINTS_PER_BLOB = 8;
const TWO_PI = Math.PI * 2;

export interface BlobSpec {
  colorVar: string;
  alpha: number;
  cx: number;
  cy: number;
  baseRadius: number;
  driftAmp: number;
  driftSpeed: number;
  driftPhase: number;
}

interface RuntimeBlob extends BlobSpec {
  wobbleAmp: number;
  points: {
    angle: number;
    speed: number;
    phase: number;
  }[];
}

const VAR_PATTERN = /var\((?<name>--[a-zA-Z0-9-]+)\)/gu;

interface BlobCanvasProps {
  specs: BlobSpec[];
  className?: string;
  /** 0-100. When provided together with toColor, each blob melts toward toColor. */
  progress?: number;
  /** A `--css-var` name, or a literal CSS color (may reference var(--x)). */
  toColor?: string;
}

/**
 * Draws soft, wobbling blob shapes on a <canvas>, animated with
 * requestAnimationFrame. `progress` (0 to 100) shifts each blob's colour toward
 * `toColor` to show upload progress: a melting colour effect, not a progress
 * bar.
 */
export function BlobCanvas({
  specs,
  className,
  progress,
  toColor: toColorProp,
}: BlobCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // progress lives in a ref so the animation loop reads it without re-running the setup
  // effect
  const progressRef = useRef(progress ?? 0);

  useEffect(() => {
    progressRef.current = progress ?? 0;
  }, [progress]);

  // Runs when specs or toColor change: builds the blobs, sizes the canvas,
  // watches for theme colour changes and starts the loop; cleans all of it up
  // on unmount.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return;
    }

    const blobs = createRuntimeBlobs(specs);
    const reduceMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)"
    ).matches;

    let width = 0;
    let height = 0;

    // Match the canvas's backing resolution to its displayed size and device pixel ratio
    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const { width: rectWidth, height: rectHeight } = rect;
      width = rectWidth;
      height = rectHeight;
      canvas.width = width * dpr;
      canvas.height = height * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(canvas);

    // Re-read each blob's CSS colour variable when the theme switches (the html
    // class changes), so colours follow the theme without remounting.
    let fromColors = new Map<string, string>();
    let toColor = "";
    const resolveColors = () => {
      const style = getComputedStyle(document.documentElement);
      fromColors = new Map(
        specs.map((s) => [s.colorVar, style.getPropertyValue(s.colorVar)])
      );
      toColor = toColorProp ? resolveColorValue(toColorProp, style) : "";
    };
    resolveColors();

    const themeObserver = new MutationObserver(resolveColors);
    themeObserver.observe(document.documentElement, {
      attributeFilter: ["class"],
    });

    // This frame's fill: the blob's own colour at 0%, shifting toward toColor
    // as progress rises, and toColor alone at 100%.
    const fillStyleFor = (blob: RuntimeBlob) => {
      const from = fromColors.get(blob.colorVar) ?? "";
      if (!toColor) {
        return from;
      }
      const ratio = Math.round(Math.min(100, Math.max(0, progressRef.current)));
      if (ratio <= 0) {
        return from;
      }
      if (ratio >= 100) {
        return toColor;
      }
      return `color-mix(in oklch, ${from} ${100 - ratio}%, ${toColor} ${ratio}%)`;
    };

    let frame = 0;
    const start = performance.now();

    // One frame: clear the canvas and redraw every blob at its current
    // wobble/drift position with "multiply" blending.
    const render = (now: number) => {
      const time = (now - start) / 1000;
      ctx.clearRect(0, 0, width, height);
      for (const blob of blobs) {
        ctx.globalCompositeOperation = "multiply";
        ctx.globalAlpha = blob.alpha;
        ctx.fillStyle = fillStyleFor(blob);
        drawBlob(ctx, blob, width, height, time);
      }
      ctx.globalAlpha = 1;
      if (!reduceMotion) {
        frame = requestAnimationFrame(render);
      }
    };

    if (reduceMotion) {
      render(start);
    } else {
      frame = requestAnimationFrame(render);
    }

    return () => {
      cancelAnimationFrame(frame);
      resizeObserver.disconnect();
      themeObserver.disconnect();
    };
    // specs/toColor define the animation setup; progress is read via ref.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [specs, toColorProp]);

  return <canvas className={className} ref={canvasRef} />;
}

/** Adds random wobble timing to each vertex of the static specs. */
function createRuntimeBlobs(specs: BlobSpec[]): RuntimeBlob[] {
  return specs.map((spec) => ({
    ...spec,
    points: Array.from({ length: POINTS_PER_BLOB }, (_, i) => ({
      angle: (i / POINTS_PER_BLOB) * TWO_PI,
      phase: Math.random() * TWO_PI,
      speed: 0.15 + Math.random() * 0.2,
    })),
    wobbleAmp: 0.16 + Math.random() * 0.08,
  }));
}

/** Resolves `--foo` shorthand or literal CSS containing var(--foo) references. */
function resolveColorValue(raw: string, style: CSSStyleDeclaration): string {
  if (raw.startsWith("--")) {
    return style.getPropertyValue(raw).trim();
  }
  return raw.replace(VAR_PATTERN, (...args) => {
    const groups = args.at(-1) as { name?: string } | undefined;
    const name = groups?.name ?? "";
    return style.getPropertyValue(name).trim();
  });
}

/** Midpoint of two points, the anchor for the smooth Bézier curves between vertices. */
function mid(a: { x: number; y: number }, b: { x: number; y: number }) {
  return {
    x: (a.x + b.x) / 2,
    y: (a.y + b.y) / 2,
  };
}

/** Computes one blob's vertices for this frame (wobble included) and fills it. */
function drawBlob(
  ctx: CanvasRenderingContext2D,
  blob: RuntimeBlob,
  width: number,
  height: number,
  time: number
) {
  const size = Math.min(width, height);
  const driftX =
    Math.sin(time * blob.driftSpeed + blob.driftPhase) * blob.driftAmp;
  const driftY =
    Math.cos(time * blob.driftSpeed * 0.8 + blob.driftPhase) * blob.driftAmp;
  const centerX = (blob.cx + driftX) * width;
  const centerY = (blob.cy + driftY) * height;
  const radius = blob.baseRadius * size;

  const vertices = blob.points.map((p) => {
    const wobble = 1 + blob.wobbleAmp * Math.sin(time * p.speed + p.phase);
    const r = radius * wobble;
    return {
      x: centerX + Math.cos(p.angle) * r,
      y: centerY + Math.sin(p.angle) * r,
    };
  });

  ctx.beginPath();
  const first = mid(vertices.at(-1) ?? vertices[0], vertices[0]);
  ctx.moveTo(first.x, first.y);
  for (let i = 0; i < vertices.length; i += 1) {
    const current = vertices[i];
    const next = vertices[(i + 1) % vertices.length];
    const midPoint = mid(current, next);
    ctx.quadraticCurveTo(current.x, current.y, midPoint.x, midPoint.y);
  }
  ctx.closePath();
  ctx.fill();
}
