import { useCallback, useEffect, useState } from "react";

/** Safari's AirPlay API, older than and richer than its `remote`. */
interface AirPlayVideo extends HTMLVideoElement {
  webkitShowPlaybackTargetPicker?: () => void;
  webkitCurrentPlaybackTargetIsWireless?: boolean;
}

interface AirPlayAvailabilityEvent extends Event {
  availability?: "available" | "not-available";
}

export type RemoteKind = "cast" | "airplay";

/**
 * Playing on another screen: AirPlay in Safari, the Remote Playback API
 * (Chromecast) elsewhere. `available` stays false until the browser has found
 * a device it could send this source to, so there is never a button that leads
 * nowhere. A blob URL (an opened encrypted share) is never available: a cast
 * device fetches the URL itself.
 */
export function useRemotePlayback(
  videoRef: React.RefObject<HTMLVideoElement | null>
) {
  const [available, setAvailable] = useState(false);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const video = videoRef.current as AirPlayVideo | null;
    if (!video) {
      return;
    }

    if (video.webkitShowPlaybackTargetPicker) {
      const handleAvailability = (event: Event) =>
        setAvailable(
          (event as AirPlayAvailabilityEvent).availability === "available"
        );
      const handleWireless = () =>
        setConnected(Boolean(video.webkitCurrentPlaybackTargetIsWireless));
      video.addEventListener(
        "webkitplaybacktargetavailabilitychanged",
        handleAvailability
      );
      video.addEventListener(
        "webkitcurrentplaybacktargetiswirelesschanged",
        handleWireless
      );
      return () => {
        video.removeEventListener(
          "webkitplaybacktargetavailabilitychanged",
          handleAvailability
        );
        video.removeEventListener(
          "webkitcurrentplaybacktargetiswirelesschanged",
          handleWireless
        );
      };
    }

    const { remote } = video;
    if (!remote) {
      return;
    }
    let watchId: number | null = null;
    let cancelled = false;
    const handleState = () => setConnected(remote.state !== "disconnected");
    for (const name of ["connecting", "connect", "disconnect"]) {
      remote.addEventListener(name, handleState);
    }
    (async () => {
      try {
        const id = await remote.watchAvailability(setAvailable);
        if (cancelled) {
          await remote.cancelWatchAvailability(id);
          return;
        }
        watchId = id;
      } catch {
        // Desktop Chrome and others refuse to watch: no cast button.
        setAvailable(false);
      }
    })();
    return () => {
      cancelled = true;
      for (const name of ["connecting", "connect", "disconnect"]) {
        remote.removeEventListener(name, handleState);
      }
      if (watchId !== null) {
        stopWatching(remote, watchId);
      }
    };
  }, [videoRef]);

  const prompt = useCallback(async () => {
    const video = videoRef.current as AirPlayVideo | null;
    if (!video) {
      return;
    }
    if (video.webkitShowPlaybackTargetPicker) {
      video.webkitShowPlaybackTargetPicker();
      return;
    }
    try {
      await video.remote.prompt();
    } catch {
      // Dismissed, or the device went away.
    }
  }, [videoRef]);

  return { available, connected, prompt };
}

async function stopWatching(remote: RemotePlayback, id: number) {
  try {
    await remote.cancelWatchAvailability(id);
  } catch {
    // Already cancelled along with the element.
  }
}

/** Which icon the button should wear: AirPlay where Safari is in charge. */
export function remoteKind(): RemoteKind {
  return "WebKitPlaybackTargetAvailabilityEvent" in window ? "airplay" : "cast";
}
