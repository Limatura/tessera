import { cn } from "@tessera/ui/lib/utils";
import { useRef, useState } from "react";

interface ScrubberProps {
  value: number;
  max: number;
  /** Arrow-key step; Page Up/Down move ten of them. */
  step: number;
  label: string;
  valueText: string;
  onChange: (value: number) => void;
  onScrubStart?: () => void;
  onScrubEnd?: () => void;
  /** Labels the position under the pointer. No tooltip without it. */
  formatHover?: (value: number) => string;
  /**
   * Ease the fill between updates. For a value that arrives a few times a
   * second, like `timeupdate`, so the bar glides instead of ticking.
   */
  smooth?: boolean;
  /** Layers drawn under the fill, inside the track (what is buffered). */
  children?: React.ReactNode;
  className?: string;
  /**
   * `overlay` (the default) is drawn on a dark picture; `surface` sits on the
   * page's own background and takes its colours.
   */
  tone?: ScrubberTone;
}

export type ScrubberTone = "overlay" | "surface";

const TONES: Record<
  ScrubberTone,
  { ring: string; track: string; hover: string; handle: string }
> = {
  overlay: {
    handle: "bg-white",
    hover: "bg-white/25",
    ring: "focus-visible:ring-white/70",
    track: "bg-white/25",
  },
  surface: {
    handle: "bg-foreground",
    hover: "bg-foreground/15",
    ring: "focus-visible:ring-ring",
    track: "bg-foreground/15",
  },
};

/**
 * A horizontal slider drawn for the player: thin on the picture, thicker and
 * with a handle while hovered, focused or dragged. Pointer capture keeps a
 * drag going past the track's ends.
 */
export function Scrubber({
  children,
  className,
  formatHover,
  label,
  max,
  onChange,
  onScrubEnd,
  onScrubStart,
  smooth = false,
  step,
  tone = "overlay",
  value,
  valueText,
}: ScrubberProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);
  /** Where the pointer is along the track, `0`–`1`. */
  const [hover, setHover] = useState<number | null>(null);
  // Nothing to scrub through yet: no duration known.
  const disabled = max <= 0;
  const ratio = disabled ? 0 : clamp01(value / max);

  const ratioAt = (clientX: number) => {
    const rect = trackRef.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) {
      return 0;
    }
    return clamp01((clientX - rect.left) / rect.width);
  };

  const endDrag = (pointerType: string) => {
    if (!dragging) {
      return;
    }
    setDragging(false);
    onScrubEnd?.();
    // A finger lifted is not hovering anywhere.
    if (pointerType !== "mouse") {
      setHover(null);
    }
  };

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || disabled) {
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    const at = ratioAt(event.clientX);
    setDragging(true);
    setHover(at);
    onScrubStart?.();
    onChange(at * max);
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    if (disabled || (!dragging && event.pointerType !== "mouse")) {
      return;
    }
    const at = ratioAt(event.clientX);
    setHover(at);
    if (dragging) {
      onChange(at * max);
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const next = disabled ? null : keyTarget(event.key, value, step, max);
    if (next === null) {
      return;
    }
    event.preventDefault();
    onChange(next);
  };

  const colors = TONES[tone];
  const easing =
    smooth && !dragging ? "duration-250 ease-linear" : "duration-0";

  return (
    <div
      aria-disabled={disabled || undefined}
      aria-label={label}
      aria-valuemax={max}
      aria-valuemin={0}
      aria-valuenow={value}
      aria-valuetext={valueText}
      className={cn(
        "group/scrubber relative flex h-5 cursor-pointer touch-none select-none items-center rounded-sm outline-none focus-visible:ring-2 data-disabled:pointer-events-none data-disabled:opacity-50",
        colors.ring,
        className
      )}
      data-disabled={disabled || undefined}
      data-dragging={dragging || undefined}
      onKeyDown={handleKeyDown}
      onLostPointerCapture={(event) => endDrag(event.pointerType)}
      onPointerDown={handlePointerDown}
      onPointerLeave={() => {
        if (!dragging) {
          setHover(null);
        }
      }}
      onPointerMove={handlePointerMove}
      onPointerUp={(event) => endDrag(event.pointerType)}
      ref={trackRef}
      // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role -- a range input cannot draw buffered ranges in its track, and iOS moves it only by dragging the thumb, not by tapping the track
      role="slider"
      tabIndex={disabled ? -1 : 0}
    >
      <div
        className={cn(
          "relative h-1 w-full overflow-hidden rounded-xs transition-[height] duration-150 group-focus-visible/scrubber:h-1.5 group-hover/scrubber:h-1.5 group-data-dragging/scrubber:h-1.5",
          colors.track
        )}
      >
        {children}
        {hover !== null && (
          <div
            className={cn("absolute inset-y-0 left-0", colors.hover)}
            style={{ width: `${hover * 100}%` }}
          />
        )}
        <div
          className={cn(
            "absolute inset-y-0 left-0 bg-primary transition-[width]",
            easing
          )}
          style={{ width: `${ratio * 100}%` }}
        />
      </div>
      <div
        className={cn(
          "pointer-events-none absolute top-1/2 h-3.5 w-1 -translate-x-1/2 -translate-y-1/2 rounded-xs opacity-0 transition-[left,opacity] group-focus-visible/scrubber:opacity-100 group-hover/scrubber:opacity-100 group-data-dragging/scrubber:opacity-100",
          colors.handle,
          easing
        )}
        style={{ left: `${ratio * 100}%` }}
      />
      {formatHover && hover !== null && (
        <div
          className="pointer-events-none absolute bottom-full mb-1.5 -translate-x-1/2 rounded-sm bg-black/80 px-1.5 py-0.5 font-medium text-white text-xs tabular-nums"
          // Kept clear of the edges, where half of it would be cut off.
          style={{
            left: `clamp(1.75rem, ${hover * 100}%, calc(100% - 1.75rem))`,
          }}
        >
          {formatHover(hover * max)}
        </div>
      )}
    </div>
  );
}

/** The value a navigation key moves to, or `null` for any other key. */
function keyTarget(key: string, value: number, step: number, max: number) {
  const PAGE = 10;
  switch (key) {
    case "ArrowLeft":
    case "ArrowDown": {
      return Math.max(0, value - step);
    }
    case "ArrowRight":
    case "ArrowUp": {
      return Math.min(max, value + step);
    }
    case "PageDown": {
      return Math.max(0, value - step * PAGE);
    }
    case "PageUp": {
      return Math.min(max, value + step * PAGE);
    }
    case "Home": {
      return 0;
    }
    case "End": {
      return max;
    }
    default: {
      return null;
    }
  }
}

function clamp01(value: number) {
  return Math.min(1, Math.max(0, value));
}
