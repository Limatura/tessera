import { useCallback, useEffect, useRef, useState } from "react";

import type { DecodeCounters } from "./media-diagnosis";
import { hasDecodedAudio, hasPicture } from "./media-diagnosis";

export interface TimeRange {
  start: number;
  end: number;
}

export interface MediaState {
  paused: boolean;
  ended: boolean;
  /** Playing was asked for but there is not enough data to go on. */
  buffering: boolean;
  /** Has played at least once, so the poster-style overlay can go. */
  started: boolean;
  currentTime: number;
  /** `0` until the metadata is in. */
  duration: number;
  /** Width over height of the picture, `null` until the metadata is in. */
  aspect: number | null;
  buffered: TimeRange[];
  volume: number;
  muted: boolean;
  rate: number;
  pictureInPicture: boolean;
  /** The file cannot be shown here: an error, or a container with no picture. */
  unplayable: boolean;
  /** Some playback and no audio decoded. See `hasDecodedAudio`. */
  silent: boolean;
}

const INITIAL: MediaState = {
  aspect: null,
  buffered: [],
  buffering: false,
  currentTime: 0,
  duration: 0,
  ended: false,
  muted: false,
  paused: true,
  pictureInPicture: false,
  rate: 1,
  silent: false,
  started: false,
  unplayable: false,
  volume: 1,
};

/** Every event that can change something `read` returns. */
const EVENTS = [
  "durationchange",
  "emptied",
  "ended",
  "enterpictureinpicture",
  "leavepictureinpicture",
  "loadedmetadata",
  "pause",
  "play",
  "playing",
  "progress",
  "ratechange",
  "seeked",
  "seeking",
  "timeupdate",
  "volumechange",
  "waiting",
  "canplay",
] as const;

/** `readyState` from which playback can go on without stalling. */
const HAVE_FUTURE_DATA = 3;

/** How much has to play before a missing audio track is worth reporting. */
const AUDIO_CHECK_AFTER_SECONDS = 1.5;

/**
 * The state of a `<video>`, read from the element on each of its events rather
 * than mirrored by hand, so it cannot drift from what is actually playing.
 */
export function useMediaState(
  videoRef: React.RefObject<HTMLVideoElement | null>
) {
  const [state, setState] = useState<MediaState>(INITIAL);
  // Sticky per source, cleared by `emptied` when a new one loads: the
  // element's own flags say only what is true at this moment.
  const started = useRef(false);
  const failed = useRef(false);
  const silent = useRef(false);
  const audioChecked = useRef(false);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) {
      return;
    }
    const update = (event?: Event) => {
      if (event?.type === "emptied") {
        started.current = false;
        failed.current = false;
        silent.current = false;
        audioChecked.current = false;
      }
      if (event?.type === "play") {
        started.current = true;
      }
      if (event?.type === "loadedmetadata" && !hasPicture(video)) {
        failed.current = true;
      }
      // Time actually played, not the position: a seek straight to the middle
      // has decoded nothing yet.
      if (
        !audioChecked.current &&
        playedSeconds(video) >= AUDIO_CHECK_AFTER_SECONDS
      ) {
        audioChecked.current = true;
        silent.current =
          hasDecodedAudio(video as HTMLVideoElement & DecodeCounters) === false;
      }
      setState(read(video, started.current, failed.current, silent.current));
    };
    const handleError = () => {
      failed.current = true;
      update();
    };

    for (const name of EVENTS) {
      video.addEventListener(name, update);
    }
    video.addEventListener("error", handleError);
    // An error before this effect ran has already fired.
    if (video.error) {
      failed.current = true;
    }
    update();
    return () => {
      for (const name of EVENTS) {
        video.removeEventListener(name, update);
      }
      video.removeEventListener("error", handleError);
    };
  }, [videoRef]);

  const play = useCallback(async () => {
    try {
      await videoRef.current?.play();
    } catch {
      // Refused (autoplay policy) or interrupted by a pause: the element's own
      // events already say it is not playing.
    }
  }, [videoRef]);

  const togglePlay = useCallback(() => {
    const video = videoRef.current;
    if (!video) {
      return;
    }
    if (video.paused || video.ended) {
      play();
      return;
    }
    video.pause();
  }, [videoRef, play]);

  const seek = useCallback(
    (seconds: number) => {
      const video = videoRef.current;
      if (!video) {
        return;
      }
      // A recording whose header never got a duration (MediaRecorder's WebM)
      // reports Infinity until it has played to the end. A relative seek still
      // works on it; only the start bounds it.
      const end = Number.isFinite(video.duration)
        ? video.duration
        : Number.POSITIVE_INFINITY;
      video.currentTime = clamp(seconds, 0, end);
    },
    [videoRef]
  );

  const seekBy = useCallback(
    (delta: number) => {
      const video = videoRef.current;
      if (video) {
        seek(video.currentTime + delta);
      }
    },
    [videoRef, seek]
  );

  const setVolume = useCallback(
    (volume: number) => {
      const video = videoRef.current;
      if (!video) {
        return;
      }
      video.volume = clamp(volume, 0, 1);
      video.muted = video.volume === 0;
    },
    [videoRef]
  );

  const toggleMute = useCallback(() => {
    const video = videoRef.current;
    if (!video) {
      return;
    }
    // Unmuting at zero volume would still be silent.
    if (video.muted && video.volume === 0) {
      video.volume = 1;
    }
    video.muted = !video.muted;
  }, [videoRef]);

  const setRate = useCallback(
    (rate: number) => {
      if (videoRef.current) {
        videoRef.current.playbackRate = rate;
      }
    },
    [videoRef]
  );

  const togglePictureInPicture = useCallback(async () => {
    const video = videoRef.current;
    if (!video) {
      return;
    }
    try {
      await (document.pictureInPictureElement === video
        ? document.exitPictureInPicture()
        : video.requestPictureInPicture());
    } catch {
      // Refused by the browser; the button stays as it was.
    }
  }, [videoRef]);

  return {
    play,
    seek,
    seekBy,
    setRate,
    setVolume,
    state,
    toggleMute,
    togglePictureInPicture,
    togglePlay,
  };
}

function read(
  video: HTMLVideoElement,
  started: boolean,
  failed: boolean,
  silent: boolean
): MediaState {
  const buffered: TimeRange[] = [];
  for (let index = 0; index < video.buffered.length; index += 1) {
    buffered.push({
      end: video.buffered.end(index),
      start: video.buffered.start(index),
    });
  }
  return {
    aspect:
      video.videoWidth > 0 && video.videoHeight > 0
        ? video.videoWidth / video.videoHeight
        : null,
    buffered,
    buffering:
      video.seeking ||
      (!video.paused && video.readyState < HAVE_FUTURE_DATA && !video.ended),
    currentTime: video.currentTime,
    duration: Number.isFinite(video.duration) ? video.duration : 0,
    ended: video.ended,
    muted: video.muted,
    paused: video.paused,
    pictureInPicture: document.pictureInPictureElement === video,
    rate: video.playbackRate,
    silent,
    started,
    unplayable: failed,
    volume: video.volume,
  };
}

function playedSeconds(video: HTMLVideoElement) {
  let total = 0;
  for (let index = 0; index < video.played.length; index += 1) {
    total += video.played.end(index) - video.played.start(index);
  }
  return total;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}
