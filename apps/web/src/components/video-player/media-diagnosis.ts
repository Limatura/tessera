/**
 * What a browser managed to decode of a video, for files whose container it
 * opens but whose codecs it may not (a Matroska file with HEVC or AC3 inside,
 * a QuickTime file with HEVC). Those do not always fail with an `error`: the
 * browser can play the audio over a black picture, or the picture in silence.
 */

/**
 * The parts of a media element read here. The counters are non-standard and
 * missing from `HTMLVideoElement`'s type, so they are optional.
 */
export interface DecodeCounters {
  videoWidth: number;
  /** Chromium: bytes of audio decoded so far. */
  webkitAudioDecodedByteCount?: number;
  /** Firefox: whether the element has audio it can play. */
  mozHasAudio?: boolean;
  /** Safari: the audio tracks the file declares. */
  audioTracks?: { length: number };
}

/** Whether there is a picture to show. Read after `loadedmetadata`. */
export function hasPicture(video: DecodeCounters): boolean {
  return video.videoWidth > 0;
}

/**
 * Whether any audio has been decoded: `true` or `false`, or `null` where the
 * browser has no way to tell. Read only after a second or so of playback,
 * since Chromium's counter starts at zero.
 *
 * A file with no audio track at all also gives `false`; nothing tells it apart
 * from a track the browser cannot decode, so the notice says both.
 */
export function hasDecodedAudio(video: DecodeCounters): boolean | null {
  if (typeof video.mozHasAudio === "boolean") {
    return video.mozHasAudio;
  }
  if (typeof video.webkitAudioDecodedByteCount === "number") {
    return video.webkitAudioDecodedByteCount > 0;
  }
  // Safari lists tracks whether or not it can decode them, but a track it
  // cannot decode fails the whole file there, so a listed track is enough.
  if (video.audioTracks) {
    return video.audioTracks.length > 0;
  }
  return null;
}
