import { useCallback, useEffect, useState } from "react";

/** iPhone Safari: no element fullscreen, only the system player's. */
interface WebkitVideo extends HTMLVideoElement {
  webkitEnterFullscreen?: () => void;
}

/**
 * Fullscreen for the whole player, so the custom controls come along. On an
 * iPhone, which can only put a `<video>` itself fullscreen, it falls back to
 * the system player.
 */
export function useFullscreen(
  containerRef: React.RefObject<HTMLElement | null>,
  videoRef: React.RefObject<HTMLVideoElement | null>
) {
  const [fullscreen, setFullscreen] = useState(false);

  useEffect(() => {
    const handleChange = () =>
      setFullscreen(
        document.fullscreenElement !== null &&
          document.fullscreenElement === containerRef.current
      );
    document.addEventListener("fullscreenchange", handleChange);
    return () => document.removeEventListener("fullscreenchange", handleChange);
  }, [containerRef]);

  const toggle = useCallback(async () => {
    try {
      if (document.fullscreenElement) {
        await document.exitFullscreen();
        return;
      }
      const container = containerRef.current;
      if (container && document.fullscreenEnabled) {
        await container.requestFullscreen();
        return;
      }
      (videoRef.current as WebkitVideo | null)?.webkitEnterFullscreen?.();
    } catch {
      // Refused, e.g. not from a user gesture; nothing changes.
    }
  }, [containerRef, videoRef]);

  return { fullscreen, toggle };
}

export function canGoFullscreen() {
  return (
    document.fullscreenEnabled ||
    "webkitEnterFullscreen" in HTMLVideoElement.prototype
  );
}
