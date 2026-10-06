export interface FilePreviewProps {
  filename: string;
  url: string;
}

export type FilePreviewComponent = (
  props: FilePreviewProps
) => React.ReactElement;
