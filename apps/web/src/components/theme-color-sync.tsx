import { useEffect } from "react";

/**
 * Keeps `<meta name="theme-color">` on the page's resolved background.
 *
 * The installed app's status bar takes this colour. The manifest carries the
 * instance default, but a visitor's own light/dark choice and theme are applied
 * on `<html>` at runtime (AppearanceSync, next-themes), so this follows those
 * attribute changes instead of guessing.
 */
export function ThemeColorSync() {
  useEffect(() => {
    let meta = document.querySelector<HTMLMetaElement>(
      'meta[name="theme-color"]'
    );
    if (!meta) {
      meta = document.createElement("meta");
      meta.name = "theme-color";
      document.head.append(meta);
    }
    const target = meta;
    const update = () => {
      target.content = toHex(getComputedStyle(document.body).backgroundColor);
    };
    update();
    const observer = new MutationObserver(update);
    observer.observe(document.documentElement, {
      attributeFilter: ["class", "data-theme", "data-accent", "data-mode"],
      attributes: true,
    });
    return () => observer.disconnect();
  }, []);

  return null;
}

/**
 * The palette is written in oklch, and an OS reading `theme-color` for the
 * status bar may not parse that. A canvas pixel gives plain sRGB.
 */
function toHex(color: string): string {
  const context = document
    .createElement("canvas")
    .getContext("2d", { willReadFrequently: true });
  if (!context) {
    return color;
  }
  context.fillStyle = color;
  context.fillRect(0, 0, 1, 1);
  const [red, green, blue] = context.getImageData(0, 0, 1, 1).data;
  return `#${[red, green, blue]
    .map((channel) => channel.toString(16).padStart(2, "0"))
    .join("")}`;
}
