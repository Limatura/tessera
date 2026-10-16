import { motion } from "motion/react";

/** One crest to the next, in px. The loop moves exactly this far, so it is seamless. */
const PERIOD = 24;
/** How far a crest reaches past the trough, in px. */
const AMPLITUDE = 6;
/** Enough crests to cover the tallest card plus the one scrolled out of view. */
const CRESTS = 8;
const HEIGHT = PERIOD * CRESTS;

/**
 * A vertical sine edge, filled on its left: from `x = 0` out to a curve that
 * swings between `AMPLITUDE` and `2 * AMPLITUDE`, one quadratic per half period.
 */
function wavePath(): string {
  const mid = AMPLITUDE * 1.5;
  const parts = [`M0 0 L${mid} 0`];
  for (let crest = 0; crest < CRESTS; crest += 1) {
    const top = crest * PERIOD;
    parts.push(
      `Q${AMPLITUDE * 2} ${top + PERIOD / 4} ${mid} ${top + PERIOD / 2}`,
      `Q${AMPLITUDE} ${top + (PERIOD * 3) / 4} ${mid} ${top + PERIOD}`
    );
  }
  parts.push(`L0 ${HEIGHT} Z`);
  return parts.join(" ");
}

const PATH = wavePath();

/**
 * The leading edge of a fill, as liquid: a sine wave drifting along it, so
 * the level looks alive rather than cut. One layer at the fill's own opacity,
 * so edge and fill read as the same water.
 *
 * Sits just right of the fill it belongs to and inherits its `currentColor`.
 * Moves by transform only, which `MotionConfig reducedMotion="user"` on the
 * admin layout stills for anyone who asked for less motion.
 */
export function WaveEdge() {
  return (
    <span
      aria-hidden
      className="absolute inset-y-0 left-full overflow-hidden"
      style={{ width: AMPLITUDE * 2 }}
    >
      <WaveLayer duration={2.4} from={-PERIOD} opacity={0.1} to={0} />
    </span>
  );
}

function WaveLayer({
  from,
  to,
  duration,
  opacity,
}: {
  from: number;
  to: number;
  duration: number;
  opacity: number;
}) {
  return (
    <motion.svg
      animate={{ y: [from, to] }}
      className="absolute top-0 left-0"
      height={HEIGHT}
      transition={{
        duration,
        ease: "linear",
        repeat: Number.POSITIVE_INFINITY,
      }}
      viewBox={`0 0 ${AMPLITUDE * 2} ${HEIGHT}`}
      width={AMPLITUDE * 2}
    >
      <path d={PATH} fill="currentColor" fillOpacity={opacity} />
    </motion.svg>
  );
}
