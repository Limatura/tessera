import { ImagePreview } from "@/routes/_site/s/$code/-components/file-previews/image-preview";
import type { FilePreviewComponent } from "@/routes/_site/s/$code/-components/file-previews/types";

interface FilePreviewRegistryEntry {
  Component: FilePreviewComponent;
  extensions: readonly string[];
}

/**
 * One entry per previewable file type, keyed by extension. To support a new
 * kind of file (PDF, video, ...), add a component next to `image-preview.tsx`
 * and register it here — nothing else on the share page needs to change.
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
    extensions: ["png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "bmp"],
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
