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

/** 在靜態設定的基礎上,為每個頂點加上隨機的抖動時間參數 */
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

const VAR_PATTERN = /var\((?<name>--[a-zA-Z0-9-]+)\)/gu;

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

/** 兩點的中點,用來作為 blob 頂點間圓滑貝茲曲線的錨點 */
function mid(a: { x: number; y: number }, b: { x: number; y: number }) {
  return {
    x: (a.x + b.x) / 2,
    y: (a.y + b.y) / 2,
  };
}

/** 計算單個 blob 這一幀的頂點位置(包含抖動),然後在 canvas 上填色 */
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

interface BlobCanvasProps {
  specs: BlobSpec[];
  className?: string;
  /** 0-100. When provided together with toColor, each blob melts toward toColor. */
  progress?: number;
  /** A `--css-var` name, or a literal CSS color (may reference var(--x)). */
  toColor?: string;
}

/**
 * 在 <canvas> 上畫一堆柔軟蠕動的 blob 形狀,靠 requestAnimationFrame 跑動畫
 * `progress` (0~100) 會把每個 blob 的顏色漸變向 `toColor`,用來表現上傳進度
 * (是個會「融化」的色彩特效,而不是進度條)
 */
export function BlobCanvas({
  specs,
  className,
  progress,
  toColor: toColorProp,
}: BlobCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // 用 ref 存 progress 讓動畫迴圈直接讀值,不用觸發下面的 setup effect
  const progressRef = useRef(progress ?? 0);

  useEffect(() => {
    progressRef.current = progress ?? 0;
  }, [progress]);

  // 每當 specs/toColor 改變時跑一次:初始化 blobs、設定 canvas 大小、
  // 監聽佈景主題顏色變化、啟動動畫迴圈,卸載時清理所有 observers 和 timer
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

    // 同步 canvas 內部解析度跟螢幕實際大小和裝置像素比
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

    // 每當佈景主題切換(html class 變化)時重新讀取每個 blob 的 CSS 顏色變數
    // 這樣不用重新掛載元件,顏色就會正確跟著主題改變
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

    // 決定目前這一幀 blob 的填充顏色:progress 0% 時用 blob 自己的顏色,
    // progress 上升時漸變向 toColor,100% 時完全變成 toColor
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

    // 單一動畫幀:清空 canvas,用 "multiply" 混合模式重畫所有 blob
    // 每個 blob 都在目前抖動/漂移位置上,產生柔和的視覺效果
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
