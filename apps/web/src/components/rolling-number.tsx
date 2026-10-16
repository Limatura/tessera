import { cn } from "@tessera/ui/lib/utils";
import { useLayoutEffect, useRef } from "react";

const DIGITS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] as const;
// Two laps of 0-9, so a digit that wraps (9 -> 0) can keep rolling forward
// into the second lap instead of spinning back through 8..1.
const STRIP = [...DIGITS, ...DIGITS];
const STEP = 100 / STRIP.length;

const translate = (position: number) => `translateY(-${position * STEP}%)`;

/** Jumps the strip to `position` with no transition. */
const snapTo = (el: HTMLElement, position: number) => {
  el.style.transition = "none";
  el.style.transform = translate(position);
  // Flush the style so restoring the transition does not animate the snap.
  el.getBoundingClientRect();
  el.style.transition = "";
};

function DigitColumn({ digit }: { digit: number }) {
  const stripRef = useRef<HTMLSpanElement>(null);
  // Index into STRIP; the shown digit is position % 10. Null until mounted.
  const positionRef = useRef<number | null>(null);

  useLayoutEffect(() => {
    const el = stripRef.current;
    const position = positionRef.current;
    if (!el) {
      return;
    }
    if (position === null) {
      positionRef.current = digit;
      snapTo(el, digit);
      return;
    }
    const current = position % DIGITS.length;
    if (digit === current) {
      return;
    }
    let target = position - current + digit;
    if (digit < current) {
      target += DIGITS.length;
    }
    if (target >= STRIP.length) {
      // Both laps look the same, so moving back one lap is invisible.
      snapTo(el, position - DIGITS.length);
      target -= DIGITS.length;
    }
    positionRef.current = target;
    el.style.transform = translate(target);
  }, [digit]);

  const handleTransitionEnd = () => {
    const el = stripRef.current;
    const position = positionRef.current;
    if (el && position !== null && position >= DIGITS.length) {
      positionRef.current = position - DIGITS.length;
      snapTo(el, positionRef.current);
    }
  };

  return (
    <span aria-hidden className="relative inline-block h-[1lh] overflow-hidden">
      <span
        className="flex flex-col transition-transform duration-300 ease-out motion-reduce:transition-none"
        onTransitionEnd={handleTransitionEnd}
        ref={stripRef}
      >
        {STRIP.map((d, index) => (
          // oxlint-disable-next-line react/no-array-index-key -- static strip, digits repeat
          <span key={index}>{d}</span>
        ))}
      </span>
    </span>
  );
}

interface RollingNumberProps {
  value: number;
  /** Read by screen readers in place of the animated digits. */
  label: string;
  suffix?: string;
  className?: string;
}

/**
 * An odometer for values that change many times a second, such as upload
 * progress. Each digit is a 0-9 strip moved by a CSS transform transition, so
 * an update only changes one style per digit: no layout reads and no new
 * animation objects, and a transition retargets smoothly when the next value
 * arrives before it ends. (NumberFlow measures every digit and starts a fresh
 * animation per update, which floods the main thread at XHR progress rates.)
 */
export function RollingNumber({
  value,
  label,
  suffix,
  className,
}: RollingNumberProps) {
  const digits = [...String(Math.max(0, Math.round(value)))].map(Number);

  return (
    <span className={cn("inline-flex tabular-nums", className)}>
      <span className="sr-only">{label}</span>
      {digits.map((digit, index) => (
        // Keyed from the right so the ones column stays the same element when
        // the number gains a digit (9 -> 10).
        <DigitColumn digit={digit} key={digits.length - index} />
      ))}
      {suffix && <span aria-hidden>{suffix}</span>}
    </span>
  );
}
