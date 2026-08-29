import { FileText, Files, Folder, Lock } from "lucide-react";

import {
  colors,
  Wordmark,
  CodeChip,
  BgGlow,
  canvas,
  IconBadge,
  clipText,
} from "./tokens";

const MAX_HEADLINE_CHARS = 24;

function LockIcon({ size = 22 }: { size?: number }) {
  return <Lock size={size} color={colors.faded} strokeWidth={2} />;
}

function MetaPill({ text }: { text: string }) {
  return (
    <div
      style={{
        padding: "8px 16px",
        borderRadius: 999,
        background: colors.panel,
        border: `1px solid ${colors.panelBorder}`,
        fontSize: 19,
        color: colors.faded,
      }}
    >
      {text}
    </div>
  );
}

// tessera's uploader supports three shapes, driven by `webkitdirectory` /
// drag-and-drop directory traversal in upload-button.tsx:
//   - a single file
//   - several unrelated files picked/dropped together (flat, no shared folder)
//   - a whole folder (path is encoded into `filename`, e.g. "src/index.ts";
//     deriveFolderName() reads the first path segment as the folder name)
type FileOgProps =
  | {
      variant?: "single";
      filename?: string;
      sizeLabel?: string;
      code?: string;
      expires?: string;
      usesLeft?: string;
      hasPassword?: boolean;
    }
  | {
      variant: "multi";
      filename?: string;
      extraFiles?: number;
      sizeLabel?: string;
      code?: string;
      expires?: string;
      usesLeft?: string;
      hasPassword?: boolean;
    }
  | {
      variant: "folder";
      folderName?: string;
      fileCount?: number;
      sizeLabel?: string;
      code?: string;
      expires?: string;
      usesLeft?: string;
      hasPassword?: boolean;
    };

export default function FileOg(props: FileOgProps) {
  const {
    code = "K3P9XQ2M",
    sizeLabel = "24.8 MB",
    expires = "Expires in 3 days",
    usesLeft = "5 retrievals left",
    hasPassword = true,
  } = props;

  const variant = props.variant ?? "single";

  let icon = FileText;
  let label = "Shared file";
  let headline = props.filename ?? "quarterly-report.pdf";
  let countPill: string | null = null;

  if (variant === "folder") {
    icon = Folder;
    label = "Shared folder";
    headline = props.folderName ?? "vacation-photos";
    countPill = `${props.fileCount ?? 24} files`;
  } else if (variant === "multi") {
    icon = Files;
    label = "Shared files";
    countPill =
      (props.extraFiles ?? 0) > 0 ? `+${props.extraFiles} more files` : null;
  }

  return (
    <div
      tw="relative flex h-full w-full flex-col justify-between"
      style={{ background: colors.ink, padding: canvas.padding }}
    >
      <BgGlow />

      <Wordmark scale={0.62} />

      <div tw="flex items-center" style={{ gap: 40 }}>
        <IconBadge icon={icon} />
        <div tw="flex flex-col" style={{ gap: 14, maxWidth: 720 }}>
          <span
            style={{
              fontSize: 20,
              fontWeight: 700,
              letterSpacing: 2,
              color: colors.primary,
              textTransform: "uppercase",
            }}
          >
            {label}
          </span>
          <h1
            style={{
              margin: 0,
              fontSize: 48,
              fontWeight: 800,
              lineHeight: 1.15,
              letterSpacing: -1,
              color: colors.paper,
              whiteSpace: "nowrap",
              overflow: "hidden",
              maxWidth: 700,
            }}
          >
            {clipText(headline, MAX_HEADLINE_CHARS)}
          </h1>
          <div tw="flex items-center" style={{ gap: 12 }}>
            <MetaPill text={sizeLabel} />
            {countPill && <MetaPill text={countPill} />}
          </div>
        </div>
      </div>

      <div tw="flex items-end justify-between">
        <div tw="flex flex-col" style={{ gap: 10 }}>
          <div tw="flex items-center" style={{ gap: 18 }}>
            <span style={{ fontSize: 19, color: colors.faded }}>{expires}</span>
            <span style={{ fontSize: 19, color: colors.faded }}>·</span>
            <span style={{ fontSize: 19, color: colors.faded }}>
              {usesLeft}
            </span>
          </div>
          {hasPassword && (
            <div tw="flex items-center" style={{ gap: 8 }}>
              <LockIcon />
              <span style={{ fontSize: 19, color: colors.faded }}>
                Password protected
              </span>
            </div>
          )}
        </div>
        <CodeChip code={code} big />
      </div>
    </div>
  );
}

export const ogOptions = { width: canvas.width, height: canvas.height };
