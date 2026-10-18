import { Button } from "@tessera/ui/components/button";
import { cn } from "@tessera/ui/lib/utils";
import {
  Airplay,
  Cast,
  FastForward,
  Loader2,
  Maximize,
  Minimize,
  Pause,
  PictureInPicture2,
  Play,
  Rewind,
  RotateCcw,
  VideoOff,
  Volume1,
  Volume2,
  VolumeX,
  X,
} from "lucide-react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { useEffect, useEffectEvent, useRef, useState } from "react";

import { EASE_OUT } from "@/lib/motion";
import * as m from "@/paraglide/messages";

import { formatTime } from "./format-time";
import { Scrubber } from "./scrubber";
import type { PlayerAction } from "./shortcuts";
import {
  ARROW_SEEK_SECONDS,
  JUMP_SEEK_SECONDS,
  VOLUME_STEP,
  shortcutFor,
} from "./shortcuts";
import { SpeedMenu } from "./speed-menu";
import { canGoFullscreen, useFullscreen } from "./use-fullscreen";
import { useIdle } from "./use-idle";
import { useMediaState } from "./use-media-state";
import { remoteKind, useRemotePlayback } from "./use-remote-playback";

/** How long the controls stay up after the last sign of the viewer. */
const IDLE_MS = 2500;
/** Two taps closer than this on one side of the picture are a double tap. */
const DOUBLE_TAP_MS = 280;
/** How long a flashed action (play, +10s, volume) stays on the picture. */
const FLASH_MS = 550;
/**
 * The player takes the picture's own shape, so there are no bars around it,
 * within these bounds: anything taller than a phone's 9:16, which is still
 * wide enough for the row of controls, or wider than 21:9 gets bars.
 */
const MIN_ASPECT = 9 / 16;
const MAX_ASPECT = 21 / 9;
const DEFAULT_ASPECT = 16 / 9;
/** The tallest the player gets in the page, as a CSS length. */
const MAX_HEIGHT = "min(70vh, 36rem)";

interface VideoPlayerProps {
  src: string;
  /** Names the video for assistive technology. */
  title: string;
}

/** Feedback for an action that has no button of its own on screen. */
interface Flash {
  id: number;
  icon: "play" | "pause" | "back" | "forward" | "volume" | "mute";
  label?: string;
}

type Side = "left" | "center" | "right";

type Media = ReturnType<typeof useMediaState>;
type Screen = ReturnType<typeof useFullscreen>;
type Remote = ReturnType<typeof useRemotePlayback>;

/**
 * A `<video>` with controls drawn for Tessera instead of the browser's: the
 * timeline in the accent colour, a speed menu, picture in picture, casting,
 * fullscreen, keyboard shortcuts while focus is inside, and double tap to skip
 * on touch.
 *
 * Nothing of the picture shows until play is pressed: a shared file can be
 * anything, and its first frame should not greet whoever opens the link.
 *
 * A file the browser opens but cannot decode is caught here too (see
 * `useMediaState`): no picture shows a "download it" panel instead of a black
 * box, and no audio leaves a note under the player.
 */
export function VideoPlayer({ src, title }: VideoPlayerProps) {
  const containerRef = useRef<HTMLElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const media = useMediaState(videoRef);
  const { state } = media;
  const screen = useFullscreen(containerRef, videoRef);
  const remote = useRemotePlayback(videoRef);
  const [menuOpen, setMenuOpen] = useState(false);
  const [scrubbing, setScrubbing] = useState(false);
  const [silentDismissed, setSilentDismissed] = useState(false);
  const [flash, setFlash] = useState<Flash | null>(null);
  const lastTap = useRef<{ side: Side; time: number } | null>(null);
  const tapTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const playing = !(state.paused || state.ended);
  const idle = useIdle(playing && !menuOpen && !scrubbing, IDLE_MS);
  const controlsShown = state.started && !idle.idle;

  useEffect(() => {
    if (!flash) {
      return;
    }
    const timer = setTimeout(() => setFlash(null), FLASH_MS);
    return () => clearTimeout(timer);
  }, [flash]);

  useEffect(
    () => () => {
      if (tapTimer.current) {
        clearTimeout(tapTimer.current);
      }
    },
    []
  );

  const show = (icon: Flash["icon"], label?: string) =>
    setFlash((previous) => ({ icon, id: (previous?.id ?? 0) + 1, label }));

  const run = (action: PlayerAction) => {
    switch (action.type) {
      case "toggle-play": {
        show(playing ? "pause" : "play");
        media.togglePlay();
        break;
      }
      case "seek-by": {
        media.seekBy(action.seconds);
        show(
          action.seconds < 0 ? "back" : "forward",
          `${Math.abs(action.seconds)}s`
        );
        break;
      }
      case "seek-to": {
        // Home, End and the digits need a duration to be a fraction of.
        if (state.duration > 0) {
          media.seek(state.duration * action.fraction);
        }
        break;
      }
      case "volume-by": {
        const current = state.muted ? 0 : state.volume;
        const next = Math.min(1, Math.max(0, current + action.delta));
        media.setVolume(next);
        show(next === 0 ? "mute" : "volume", `${Math.round(next * 100)}%`);
        break;
      }
      case "toggle-mute": {
        show(state.muted ? "volume" : "mute");
        media.toggleMute();
        break;
      }
      default: {
        screen.toggle();
      }
    }
  };

  const handleKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (event.altKey || event.ctrlKey || event.metaKey) {
      return;
    }
    const target = event.target instanceof Element ? event.target : null;
    // The speed menu moves through its items with the arrows itself.
    if (target?.closest('[role="menu"]')) {
      return;
    }
    const action = shortcutFor(event.key, {
      onButton: Boolean(target?.closest("button")),
      onSlider: Boolean(target?.closest('[role="slider"]')),
    });
    if (action) {
      event.preventDefault();
      run(action);
      idle.wake();
    }
  });

  // A native listener, not `onKeyDown`: the container is a figure, not a
  // control, and the keys arrive from whichever control inside has focus.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const listener = (event: KeyboardEvent) => handleKeyDown(event);
    container.addEventListener("keydown", listener);
    return () => container.removeEventListener("keydown", listener);
  }, []);

  const handleSingleTap = () => {
    if (controlsShown) {
      run({ type: "toggle-play" });
    }
    idle.wake();
  };

  // A finger on the picture: the first tap brings the controls back, one with
  // the controls up plays or pauses, and two quick taps on either side skip.
  const handleTouchTap = (clientX: number, surface: HTMLElement) => {
    const rect = surface.getBoundingClientRect();
    const side = sideOf((clientX - rect.left) / rect.width);
    const now = performance.now();
    const previous = lastTap.current;
    lastTap.current = { side, time: now };
    if (tapTimer.current) {
      clearTimeout(tapTimer.current);
    }

    const doubleTap =
      side !== "center" &&
      previous?.side === side &&
      now - previous.time < DOUBLE_TAP_MS;
    if (doubleTap) {
      run({
        seconds: side === "left" ? -JUMP_SEEK_SECONDS : JUMP_SEEK_SECONDS,
        type: "seek-by",
      });
      return;
    }
    // In the middle there is no double tap to wait for.
    if (side === "center") {
      handleSingleTap();
      return;
    }
    tapTimer.current = setTimeout(handleSingleTap, DOUBLE_TAP_MS);
  };

  const handleSurfaceClick = (event: React.MouseEvent<HTMLButtonElement>) => {
    const { pointerType } = event.nativeEvent as PointerEvent;
    if (pointerType === "touch" || pointerType === "pen") {
      handleTouchTap(event.clientX, event.currentTarget);
      return;
    }
    run({ type: "toggle-play" });
  };

  if (state.unplayable) {
    return (
      <div className="flex flex-col items-center gap-2 rounded-md border border-border/60 bg-muted/20 p-8 text-center">
        <VideoOff className="size-8 text-muted-foreground" />
        <p className="text-muted-foreground text-sm">{m.video_unplayable()}</p>
      </div>
    );
  }

  return (
    <MotionConfig reducedMotion="user">
      <div className="flex flex-col gap-2">
        <figure
          aria-label={m.video_player()}
          className={cn(
            "group/player relative overflow-hidden text-white",
            screen.fullscreen
              ? "flex w-full items-center bg-black"
              : "mx-auto rounded-md",
            idle.idle && "cursor-none"
          )}
          style={screen.fullscreen ? undefined : sizeFor(state.aspect)}
          // Keyboard focus only: a tap focuses the picture too, and waking the
          // controls before its click lands would turn "show the controls"
          // into "pause".
          onFocusCapture={(event) => {
            if (event.target.matches(":focus-visible")) {
              idle.wake();
            }
          }}
          onPointerLeave={(event) => {
            if (event.pointerType === "mouse" && playing && !menuOpen) {
              idle.sleep();
            }
          }}
          onPointerMove={(event) => {
            if (event.pointerType === "mouse") {
              idle.wake();
            }
          }}
          ref={containerRef}
        >
          {/* oxlint-disable-next-line jsx-a11y/media-has-caption -- an uploaded file comes with no caption track to offer */}
          <video
            aria-label={title}
            className="block size-full bg-black object-contain"
            playsInline
            preload="metadata"
            ref={videoRef}
            src={src}
          />

          <button
            aria-label={playing ? m.video_pause() : m.video_play()}
            className="absolute inset-0 cursor-[inherit] outline-none"
            onClick={handleSurfaceClick}
            onDoubleClick={(event) => {
              if ((event.nativeEvent as PointerEvent).pointerType === "mouse") {
                screen.toggle();
              }
            }}
            tabIndex={-1}
            type="button"
          />

          <CenterOverlay
            buffering={state.buffering}
            ended={state.ended}
            flash={flash}
            onPlay={() => {
              media.play();
              idle.wake();
            }}
            started={state.started}
          />

          {remote.connected && (
            <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-2 bg-black text-sm text-white/80">
              <RemoteIcon className="size-8" />
              {m.video_casting()}
            </div>
          )}

          {!(state.started || remote.connected) && (
            <Cover
              duration={state.duration}
              onPlay={() => {
                media.play();
                idle.wake();
              }}
              remote={remote}
            />
          )}

          <ControlBar
            container={containerRef}
            media={media}
            onMenuOpenChange={setMenuOpen}
            onScrubbingChange={setScrubbing}
            playing={playing}
            remote={remote}
            screen={screen}
            visible={controlsShown}
          />
        </figure>

        {state.silent && !silentDismissed && (
          <div className="flex items-start gap-2 rounded-md border border-border/60 bg-muted/30 py-2 pr-1 pl-3 text-muted-foreground text-xs">
            <VolumeX className="mt-0.5 size-3.5 shrink-0" />
            <p className="flex-1 leading-relaxed">{m.video_no_audio()}</p>
            <Button
              aria-label={m.video_dismiss()}
              onClick={() => setSilentDismissed(true)}
              size="icon-xs"
              type="button"
              variant="ghost"
            >
              <X />
            </Button>
          </div>
        )}
      </div>
    </MotionConfig>
  );
}

/** The timeline and the row of buttons under it, sliding away when idle. */
function ControlBar({
  container,
  media,
  onMenuOpenChange,
  onScrubbingChange,
  playing,
  remote,
  screen,
  visible,
}: {
  container: React.RefObject<HTMLElement | null>;
  media: Media;
  onMenuOpenChange: (open: boolean) => void;
  onScrubbingChange: (scrubbing: boolean) => void;
  playing: boolean;
  remote: Remote;
  screen: Screen;
  visible: boolean;
}) {
  const { state } = media;
  const volume = state.muted ? 0 : state.volume;

  return (
    <div
      className={cn(
        "absolute inset-x-0 bottom-0 bg-linear-to-t from-black/80 via-black/40 to-transparent px-3 pt-12 pb-1.5 transition-[opacity,translate] duration-200",
        visible
          ? "translate-y-0 opacity-100"
          : "pointer-events-none translate-y-2 opacity-0"
      )}
      // Hidden controls are skipped by Tab until they come back.
      inert={!visible}
    >
      <Scrubber
        formatHover={(seconds) => formatTime(seconds, state.duration)}
        label={m.video_seek()}
        max={state.duration}
        onChange={media.seek}
        onScrubEnd={() => onScrubbingChange(false)}
        onScrubStart={() => onScrubbingChange(true)}
        smooth={playing}
        step={ARROW_SEEK_SECONDS}
        value={state.currentTime}
        valueText={m.video_time_of({
          current: formatTime(state.currentTime, state.duration),
          duration: formatTime(state.duration),
        })}
      >
        {state.buffered.map((range) => (
          <div
            className="absolute inset-y-0 bg-white/30"
            key={range.start}
            style={{
              left: percent(range.start, state.duration),
              width: percent(range.end - range.start, state.duration),
            }}
          />
        ))}
      </Scrubber>

      <div className="flex items-center gap-0.5">
        <ControlButton
          label={playing ? m.video_pause() : m.video_play()}
          onClick={media.togglePlay}
        >
          {playing ? (
            <Pause className="fill-current" />
          ) : (
            <Play className="fill-current" />
          )}
        </ControlButton>

        <div className="group/volume flex items-center">
          <ControlButton
            label={state.muted ? m.video_unmute() : m.video_mute()}
            onClick={media.toggleMute}
          >
            <VolumeIcon volume={volume} />
          </ControlButton>
          {/* Phones set the volume with their buttons; iOS ignores `volume` anyway. */}
          <div className="hidden w-0 overflow-hidden opacity-0 transition-[width,opacity] duration-200 group-focus-within/volume:w-20 group-focus-within/volume:opacity-100 group-hover/volume:w-20 group-hover/volume:opacity-100 pointer-fine:block">
            <Scrubber
              className="mx-2 w-16"
              label={m.video_volume()}
              max={1}
              onChange={media.setVolume}
              step={VOLUME_STEP}
              value={volume}
              valueText={`${Math.round(volume * 100)}%`}
            />
          </div>
        </div>

        <span className="px-1.5 text-white/90 text-xs tabular-nums">
          {formatTime(state.currentTime, state.duration)}
          {state.duration > 0 && (
            <span className="text-white/50">
              {" / "}
              {formatTime(state.duration)}
            </span>
          )}
        </span>

        <span className="flex-1" />

        <SpeedMenu
          container={container}
          onOpenChange={onMenuOpenChange}
          onRateChange={media.setRate}
          rate={state.rate}
        />
        {remote.available && (
          <ControlButton
            label={m.video_cast()}
            onClick={remote.prompt}
            pressed={remote.connected}
          >
            <RemoteIcon />
          </ControlButton>
        )}
        {document.pictureInPictureEnabled && (
          <ControlButton
            label={m.video_pip()}
            onClick={media.togglePictureInPicture}
            pressed={state.pictureInPicture}
          >
            <PictureInPicture2 />
          </ControlButton>
        )}
        {canGoFullscreen() && (
          <ControlButton
            label={
              screen.fullscreen
                ? m.video_exit_fullscreen()
                : m.video_fullscreen()
            }
            onClick={screen.toggle}
          >
            {screen.fullscreen ? <Minimize /> : <Maximize />}
          </ControlButton>
        )}
      </div>
    </div>
  );
}

/**
 * What sits in the middle of the picture once it is showing: replay at the
 * end, a spinner while it waits for data, and otherwise the last flashed
 * action.
 */
function CenterOverlay({
  buffering,
  ended,
  flash,
  onPlay,
  started,
}: {
  buffering: boolean;
  ended: boolean;
  flash: Flash | null;
  onPlay: () => void;
  started: boolean;
}) {
  return (
    <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
      {ended && (
        <BigButton label={m.video_replay()} onClick={onPlay}>
          <RotateCcw className="size-6" />
        </BigButton>
      )}
      {buffering && !ended && (
        <motion.div
          animate={{ opacity: 1 }}
          initial={{ opacity: 0 }}
          // Short stalls come and go without a spinner flashing up.
          transition={{ delay: 0.3, duration: 0.2 }}
        >
          <Loader2 className="size-8 animate-spin text-white/90" />
        </motion.div>
      )}
      <AnimatePresence>
        {/* Not over the play or replay button, nor the spinner. */}
        {flash && started && !ended && !buffering && (
          <motion.div
            animate={{ opacity: 1, scale: 1 }}
            className="absolute flex items-center gap-1.5 rounded-lg bg-black/55 px-3 py-2 font-medium text-sm text-white tabular-nums backdrop-blur-md"
            exit={{ opacity: 0, scale: 1.08 }}
            initial={{ opacity: 0, scale: 0.9 }}
            key={flash.id}
            transition={{ duration: 0.18, ease: EASE_OUT }}
          >
            <FlashIcon icon={flash.icon} />
            {flash.label}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/**
 * Stands in for the picture until the first play, in the page's own colours
 * rather than a black box. The whole of it is the play button.
 */
function Cover({
  duration,
  onPlay,
  remote,
}: {
  duration: number;
  onPlay: () => void;
  remote: Remote;
}) {
  return (
    <div className="absolute inset-0 bg-muted text-muted-foreground">
      <button
        aria-label={m.video_play()}
        className="group/cover absolute inset-0 flex flex-col items-center justify-center gap-3 outline-none"
        onClick={onPlay}
        type="button"
      >
        <span className="flex size-14 items-center justify-center rounded-lg bg-background text-foreground ring-1 ring-foreground/10 transition-[scale] group-hover/cover:scale-105 group-focus-visible/cover:ring-2 group-focus-visible/cover:ring-ring group-active/cover:scale-95">
          <Play className="ml-0.5 size-6 fill-current" />
        </span>
        <span className="px-4 text-center text-xs">{m.video_cover_hint()}</span>
      </button>
      {duration > 0 && (
        <span className="pointer-events-none absolute right-2 bottom-2 rounded-sm bg-background/80 px-1.5 py-0.5 font-medium text-foreground text-xs tabular-nums">
          {formatTime(duration)}
        </span>
      )}
      {remote.available && (
        <Button
          aria-label={m.video_cast()}
          className="absolute top-2 right-2"
          onClick={remote.prompt}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <RemoteIcon />
        </Button>
      )}
    </div>
  );
}

function RemoteIcon({ className }: { className?: string }) {
  return remoteKind() === "airplay" ? (
    <Airplay className={className} />
  ) : (
    <Cast className={className} />
  );
}

function BigButton({
  children,
  label,
  onClick,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      className="pointer-events-auto flex size-14 items-center justify-center rounded-lg bg-black/55 text-white ring-1 ring-white/15 backdrop-blur-md outline-none transition-[background-color,scale] hover:scale-105 hover:bg-black/70 focus-visible:ring-2 focus-visible:ring-white/70 active:scale-95"
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}

function FlashIcon({ icon }: { icon: Flash["icon"] }) {
  const className = "size-5";
  switch (icon) {
    case "play": {
      return <Play className={cn(className, "fill-current")} />;
    }
    case "pause": {
      return <Pause className={cn(className, "fill-current")} />;
    }
    case "back": {
      return <Rewind className={cn(className, "fill-current")} />;
    }
    case "forward": {
      return <FastForward className={cn(className, "fill-current")} />;
    }
    case "mute": {
      return <VolumeX className={className} />;
    }
    default: {
      return <Volume2 className={className} />;
    }
  }
}

function VolumeIcon({ volume }: { volume: number }) {
  const HALF = 0.5;
  if (volume === 0) {
    return <VolumeX />;
  }
  return volume < HALF ? <Volume1 /> : <Volume2 />;
}

function ControlButton({
  children,
  label,
  onClick,
  pressed,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
  pressed?: boolean;
}) {
  return (
    <button
      aria-label={label}
      aria-pressed={pressed}
      className="flex size-8 shrink-0 items-center justify-center rounded-md text-white/90 outline-none transition-colors hover:bg-white/10 hover:text-white focus-visible:ring-2 focus-visible:ring-white/70 aria-pressed:text-primary [&_svg]:size-4.5"
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The player's box in the page: the picture's shape, as wide as the card
 * allows and no taller than `MAX_HEIGHT`, so it never needs bars.
 */
function sizeFor(aspect: number | null): React.CSSProperties {
  const ratio = Math.min(
    MAX_ASPECT,
    Math.max(MIN_ASPECT, aspect ?? DEFAULT_ASPECT)
  );
  return {
    aspectRatio: ratio,
    width: `min(100%, calc(${MAX_HEIGHT} * ${ratio}))`,
  };
}

function sideOf(ratio: number): Side {
  const THIRD = 1 / 3;
  if (ratio < THIRD) {
    return "left";
  }
  return ratio > 1 - THIRD ? "right" : "center";
}

function percent(seconds: number, duration: number) {
  return duration > 0 ? `${(seconds / duration) * 100}%` : "0%";
}
