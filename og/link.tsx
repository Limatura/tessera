import { Link2 } from "lucide-react";

import {
  colors,
  Wordmark,
  CodeChip,
  BgGlow,
  canvas,
  IconBadge,
  clipText,
} from "./tokens";

const MAX_URL_CHARS = 46;

export default function LinkOg({
  url = "https://example.com/blog/why-tessera-exists",
  code = "9H2M-VXQ8",
  expires = "Expires in 7 days",
}: {
  url?: string;
  code?: string;
  expires?: string;
}) {
  return (
    <div
      tw="relative flex h-full w-full flex-col justify-between"
      style={{ background: colors.ink, padding: canvas.padding }}
    >
      <BgGlow />

      <Wordmark scale={0.62} />

      <div tw="flex flex-col items-start" style={{ gap: 26 }}>
        <div tw="flex items-center" style={{ gap: 24 }}>
          <IconBadge icon={Link2} />
          <span
            style={{
              fontSize: 20,
              fontWeight: 700,
              letterSpacing: 2,
              color: colors.primary,
              textTransform: "uppercase",
            }}
          >
            Shared link
          </span>
        </div>
        <div
          style={{
            padding: "22px 30px",
            borderRadius: 16,
            background: colors.panel,
            border: `1px solid ${colors.panelBorder}`,
            maxWidth: 980,
          }}
        >
          <span
            style={{
              fontFamily: "monospace",
              fontSize: 34,
              fontWeight: 600,
              color: colors.paper,
              whiteSpace: "nowrap",
              overflow: "hidden",
              display: "block",
              maxWidth: 920,
            }}
          >
            {clipText(url, MAX_URL_CHARS)}
          </span>
        </div>
      </div>

      <div tw="flex items-end justify-between">
        <span style={{ fontSize: 19, color: colors.faded }}>{expires}</span>
        <CodeChip code={code} big />
      </div>
    </div>
  );
}

export const ogOptions = { width: canvas.width, height: canvas.height };
