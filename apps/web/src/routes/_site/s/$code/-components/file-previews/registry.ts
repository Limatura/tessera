import { AudioPreview } from "@/routes/_site/s/$code/-components/file-previews/audio-preview";
import { ImagePreview } from "@/routes/_site/s/$code/-components/file-previews/image-preview";
import { SvgPreview } from "@/routes/_site/s/$code/-components/file-previews/svg-preview";
import type { FilePreviewComponent } from "@/routes/_site/s/$code/-components/file-previews/types";
import { VideoPreview } from "@/routes/_site/s/$code/-components/file-previews/video-preview";

interface FilePreviewRegistryEntry {
  Component: FilePreviewComponent;
  extensions: readonly string[];
}

/**
 * One entry per previewable file type, keyed by extension. To support a new
 * kind of file (PDF, ...), add a component next to `image-preview.tsx`
 * and register it here; nothing else on the share page needs to change.
 *
 * The download endpoint always serves `Content-Disposition: attachment`,
 * which browsers only honor for navigations. `<img>`/`<video>`/`<audio>`
 * fetch as sub-resources, not navigations, so they render inline fine; an
 * `<iframe>` (e.g. for PDFs) *is* a navigation and would just download, so
 * that needs either a backend change or fetching the bytes as a blob URL.
 */
const REGISTRY: readonly FilePreviewRegistryEntry[] = [
  {
    Component: ImagePreview,
    extensions: ["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"],
  },
  // SVG cannot go straight into <img src=download URL>: the endpoint sends
  // octet-stream + nosniff, and browsers sniff raster formats but not SVG.
  {
    Component: SvgPreview,
    extensions: ["svg"],
  },
  // Listed by container, not by what will decode: an MKV or MOV can carry
  // codecs a browser cannot play. The player tries, and falls back to a
  // "download it" panel when no picture comes out.
  {
    Component: VideoPreview,
    extensions: ["mp4", "m4v", "webm", "mov", "mkv", "ogv"],
  },
  // `ogg` is usually audio; an Ogg video in it still plays here, without the
  // picture.
  {
    Component: AudioPreview,
    extensions: [
      "mp3",
      "m4a",
      "aac",
      "wav",
      "flac",
      "ogg",
      "oga",
      "opus",
      "weba",
    ],
  },
];

export function resolveFilePreview(
  filename: string
): FilePreviewComponent | null {
  const extension = filename.split(".").pop()?.toLowerCase();
  if (!extension) {
    return null;
  }
  return (
    REGISTRY.find((entry) => entry.extensions.includes(extension))?.Component ??
    null
  );
}
