import { VideoPlayer } from "@/components/video-player/video-player";
import type { FilePreviewProps } from "@/routes/_site/s/$code/-components/file-previews/types";

export function VideoPreview({ filename, url }: FilePreviewProps) {
  return <VideoPlayer src={url} title={filename} />;
}
