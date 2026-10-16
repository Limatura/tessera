import { motion, useReducedMotion } from "motion/react";
import { useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { encode } from "uqr";

// Spec requires a 4-module quiet zone around the symbol.
const QUIET_ZONE = 2;
// Seconds of stagger per module of distance from the centre.
const RIPPLE_STEP = 0.018;
// Both stops are the accent pulled dark (OKLCH lightness <= 0.38, about 8:1
// against white) so the gradient never costs a scanner its contrast. The
// plain hex is the fallback for browsers without relative colour syntax.
const INK_START = "oklch(from var(--primary) 0.38 c h)";
const INK_END = "oklch(from var(--primary) 0.3 c calc(h + 50))";

interface AnimatedQRCodeProps {
  value: string;
  size: number;
  bgColor?: string;
  /** Pop the modules in on mount; off renders the finished code at once. */
  animated?: boolean;
  /** Delay before the ripple starts, in seconds, counted from when `play` turns on. */
  delay?: number;
  /** Holds the modules hidden until true, so the ripple can wait for its container to show. */
  play?: boolean;
  /**
   * A wave already moving across the screen (viewport px) that the ripple should
   * continue: it starts at the code's point nearest the wave's centre, at the
   * moment `arrival(distance)` (seconds after `play`) says the wave gets there.
   * Without it the ripple starts from the centre after `delay`.
   */
  wave?: { x: number; y: number; arrival: (distance: number) => number };
}

/** Where the ripple starts, in module coordinates, and when, in seconds. */
interface RippleStart {
  x: number;
  y: number;
  at: number;
}

export function AnimatedQRCode({
  value,
  size,
  bgColor = "#ffffff",
  animated = true,
  delay = 0,
  play = true,
  wave,
}: AnimatedQRCodeProps) {
  const reduced = useReducedMotion();
  const gradientId = useId();
  const svgRef = useRef<SVGSVGElement>(null);

  const { cells, count } = useMemo(() => {
    const { data, size: modules } = encode(value, { ecc: "M", border: 0 });
    const dark = data.flatMap((row, y) =>
      row.flatMap((on, x) => (on ? [{ x, y }] : []))
    );
    return { cells: dark, count: modules };
  }, [value]);

  const total = count + QUIET_ZONE * 2;
  const centre = (count - 1) / 2;
  // With a wave, unknown until measured; the modules hold still until then
  const [start, setStart] = useState<RippleStart | null>(
    wave ? null : { at: delay, x: centre, y: centre }
  );

  // Measured as play turns on, before paint, so the modules never animate from
  // a stale start
  useLayoutEffect(() => {
    const svg = svgRef.current;
    if (!(play && wave && svg)) {
      return;
    }
    const rect = svg.getBoundingClientRect();
    const nearestX = Math.min(Math.max(wave.x, rect.left), rect.right);
    const nearestY = Math.min(Math.max(wave.y, rect.top), rect.bottom);
    const scale = total / rect.width;
    setStart({
      at: wave.arrival(Math.hypot(nearestX - wave.x, nearestY - wave.y)),
      x: (nearestX - rect.left) * scale - QUIET_ZONE,
      y: (nearestY - rect.top) * scale - QUIET_ZONE,
    });
    // Only the moment play turns on matters; the wave is fixed by then
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [play]);

  return (
    <svg
      height={size}
      ref={svgRef}
      viewBox={`0 0 ${total} ${total}`}
      width={size}
    >
      <title>QR code</title>
      <defs>
        <linearGradient
          gradientUnits="userSpaceOnUse"
          id={gradientId}
          x1={0}
          x2={count}
          y1={0}
          y2={count}
        >
          <stop
            offset="0"
            stopColor="#1f1f1f"
            style={{ stopColor: INK_START }}
          />
          <stop offset="1" stopColor="#111111" style={{ stopColor: INK_END }} />
        </linearGradient>
      </defs>
      <rect fill={bgColor} height={total} width={total} />
      <g transform={`translate(${QUIET_ZONE} ${QUIET_ZONE})`}>
        {cells.map(({ x, y }) => (
          <motion.rect
            animate={play && start ? { opacity: 1, scale: 1 } : undefined}
            fill={`url(#${gradientId})`}
            height={1.02}
            initial={reduced || !animated ? false : { opacity: 0, scale: 0 }}
            key={`${x}-${y}`}
            style={{
              transformBox: "fill-box",
              transformOrigin: "center",
            }}
            transition={{
              delay: start
                ? start.at + Math.hypot(x - start.x, y - start.y) * RIPPLE_STEP
                : 0,
              type: "spring",
              stiffness: 420,
              damping: 18,
            }}
            width={1.02}
            x={x}
            y={y}
          />
        ))}
      </g>
    </svg>
  );
}
