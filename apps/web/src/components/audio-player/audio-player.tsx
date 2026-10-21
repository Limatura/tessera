import {
  AudioLines,
  FileAudio,
  Pause,
  Play,
  RotateCcw,
  RotateCw,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { MotionConfig, motion } from "motion/react";
import { useEffect, useEffectEvent, useRef } from "react";

import { formatTime } from "@/components/video-player/format-time";
import { Scrubber } from "@/components/video-player/scrubber";
import type { PlayerAction } from "@/components/video-player/shortcuts";
import {
  ARROW_SEEK_SECONDS,
  JUMP_SEEK_SECONDS,
  VOLUME_STEP,
  shortcutFor,
} from "@/components/video-player/shortcuts";
import { SpeedMenu } from "@/components/video-player/speed-menu";
import { useMediaState } from "@/components/video-player/use-media-state";
import * as m from "@/paraglide/messages";

interface AudioPlayerProps {
  src: string;
  /** Shown on the card and names the audio for assistive technology. */
  title: string;
}

/** Heights the bars of the playing indicator move between, as fractions. */
const BARS = [
  [0.35, 1, 0.5],
  [0.8, 0.3, 0.9],
  [0.5, 0.85, 0.25],
  [0.95, 0.45, 0.7],
] as const;

/**
 * An `<audio>` with controls drawn for Tessera, sharing the video player's
 * parts (timeline, speed menu, keyboard shortcuts) but laid out as a card in
 * the page's own colours: there is no picture to draw them over.
 *
 * A file the browser cannot decode either fails with an `error` or plays in
 * silence; both end in a "download it" panel, since a silent audio preview is
 * worth nothing.
 */
export function AudioPlayer({ src, title }: AudioPlayerProps) {
  const containerRef = useRef<HTMLElement>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const media = useMediaState(audioRef);
  const { state } = media;
  const playing = !(state.paused || state.ended);
  const volume = state.muted ? 0 : state.volume;

  const run = (action: PlayerAction) => {
    switch (action.type) {
      case "toggle-play": {
        media.togglePlay();
        break;
      }
      case "seek-by": {
        media.seekBy(action.seconds);
        break;
      }
      case "seek-to": {
        if (state.duration > 0) {
          media.seek(state.duration * action.fraction);
        }
        break;
      }
      case "volume-by": {
        media.setVolume(Math.min(1, Math.max(0, volume + action.delta)));
        break;
      }
      case "toggle-mute": {
        media.toggleMute();
        break;
      }
      default:
    }
  };

  const handleKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (event.altKey || event.ctrlKey || event.metaKey) {
      return;
    }
    const target = event.target instanceof Element ? event.target : null;
    if (target?.closest('[role="menu"]')) {
      return;
    }
    const action = shortcutFor(event.key, {
      onButton: Boolean(target?.closest("button")),
      onSlider: Boolean(target?.closest('[role="slider"]')),
    });
    if (action && action.type !== "toggle-fullscreen") {
      event.preventDefault();
      run(action);
    }
  });

  // Same as the video player: keys arrive from whichever control has focus.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const listener = (event: KeyboardEvent) => handleKeyDown(event);
    container.addEventListener("keydown", listener);
    return () => container.removeEventListener("keydown", listener);
  }, []);

  if (state.unplayable || state.silent) {
    return (
      <div className="flex flex-col items-center gap-2 rounded-md border border-border/60 bg-muted/20 p-8 text-center">
        <FileAudio className="size-8 text-muted-foreground" />
        <p className="text-muted-foreground text-sm">{m.audio_unplayable()}</p>
      </div>
    );
  }

  return (
    <MotionConfig reducedMotion="user">
      <figure
        aria-label={m.audio_player()}
        className="flex flex-col gap-3 rounded-lg border border-border/60 bg-muted/20 p-3 sm:p-4"
        ref={containerRef}
      >
        {/* oxlint-disable-next-line jsx-a11y/media-has-caption -- an uploaded file comes with no caption track to offer */}
        <audio preload="metadata" ref={audioRef} src={src} />

        <div className="flex items-center gap-3">
          <button
            aria-label={playing ? m.video_pause() : m.video_play()}
            className="flex size-12 shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground outline-none transition-[scale,box-shadow] hover:scale-105 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background active:scale-95"
            onClick={media.togglePlay}
            type="button"
          >
            {playing ? (
              <Pause className="size-5 fill-current" />
            ) : (
              <Play className="ml-0.5 size-5 fill-current" />
            )}
          </button>

          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <figcaption className="truncate font-medium text-sm" title={title}>
              {title}
            </figcaption>
            <span className="text-muted-foreground text-xs tabular-nums">
              {formatTime(state.currentTime, state.duration)}
              {state.duration > 0 && ` / ${formatTime(state.duration)}`}
            </span>
          </div>

          <PlayingBars buffering={state.buffering} playing={playing} />
        </div>

        <Scrubber
          formatHover={(seconds) => formatTime(seconds, state.duration)}
          label={m.video_seek()}
          max={state.duration}
          onChange={media.seek}
          smooth={playing}
          step={ARROW_SEEK_SECONDS}
          tone="surface"
          value={state.currentTime}
          valueText={m.video_time_of({
            current: formatTime(state.currentTime, state.duration),
            duration: formatTime(state.duration),
          })}
        >
          {state.buffered.map((range) => (
            <div
              className="absolute inset-y-0 bg-foreground/15"
              key={range.start}
              style={{
                left: percent(range.start, state.duration),
                width: percent(range.end - range.start, state.duration),
              }}
            />
          ))}
        </Scrubber>

        <div className="-mx-1 flex items-center gap-0.5">
          <ControlButton
            label={m.audio_back({ seconds: JUMP_SEEK_SECONDS })}
            onClick={() => media.seekBy(-JUMP_SEEK_SECONDS)}
          >
            <RotateCcw />
          </ControlButton>
          <ControlButton
            label={m.audio_forward({ seconds: JUMP_SEEK_SECONDS })}
            onClick={() => media.seekBy(JUMP_SEEK_SECONDS)}
          >
            <RotateCw />
          </ControlButton>

          <span className="flex-1" />

          <div className="group/volume flex items-center">
            {/* Phones set the volume with their buttons; iOS ignores `volume` anyway. */}
            <div className="hidden w-0 overflow-hidden opacity-0 transition-[width,opacity] duration-200 group-focus-within/volume:w-20 group-focus-within/volume:opacity-100 group-hover/volume:w-20 group-hover/volume:opacity-100 pointer-fine:block">
              <Scrubber
                className="mx-2 w-16"
                label={m.video_volume()}
                max={1}
                onChange={media.setVolume}
                step={VOLUME_STEP}
                tone="surface"
                value={volume}
                valueText={`${Math.round(volume * 100)}%`}
              />
            </div>
            <ControlButton
              label={state.muted ? m.video_unmute() : m.video_mute()}
              onClick={media.toggleMute}
            >
              <VolumeIcon volume={volume} />
            </ControlButton>
          </div>

          <SpeedMenu
            container={containerRef}
            onRateChange={media.setRate}
            rate={state.rate}
            tone="surface"
          />
        </div>
      </figure>
    </MotionConfig>
  );
}

/**
 * A small equalizer that moves while sound is coming out, standing in for the
 * picture a video would have. It is decoration, not a reading of the audio:
 * drawing a real waveform would mean downloading and decoding the whole file.
 */
function PlayingBars({
  buffering,
  playing,
}: {
  buffering: boolean;
  playing: boolean;
}) {
  const active = playing && !buffering;
  if (!(playing || buffering)) {
    return <AudioLines aria-hidden className="size-5 text-muted-foreground" />;
  }
  return (
    <div aria-hidden className="flex h-5 items-end gap-0.5">
      {BARS.map((heights, index) => (
        <motion.span
          animate={{ scaleY: active ? [...heights, heights[0]] : 0.2 }}
          className="h-full w-1 origin-bottom rounded-full bg-primary"
          // oxlint-disable-next-line react/no-array-index-key -- a fixed list that never reorders
          key={index}
          transition={
            active
              ? {
                  duration: 0.9 + index * 0.15,
                  ease: "easeInOut",
                  repeat: Number.POSITIVE_INFINITY,
                }
              : { duration: 0.2 }
          }
        />
      ))}
    </div>
  );
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
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      className="flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-4"
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}

function percent(seconds: number, duration: number) {
  return duration > 0 ? `${(seconds / duration) * 100}%` : "0%";
}
