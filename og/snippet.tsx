import { createHighlighter } from "shiki";
import type { ThemedToken } from "shiki";

import { colors, Wordmark, CodeChip, BgGlow, canvas, clipText } from "./tokens";

// Three muted "tile" colors for the window dots — a nod to tessera (mosaic tile)
const dotColors = ["#d97757", "#c9a227", "#7a8c6f"];

// Bounds so a huge paste can't blow out the fixed 1200x630 canvas:
// only so many lines fit vertically, only so many characters fit horizontally.
const MAX_LINES = 9;
const MAX_CHARS_PER_LINE = 64;
const MAX_TITLE_CHARS = 36;

let highlighterPromise: ReturnType<typeof createHighlighter> | null = null;
function getHighlighter() {
  highlighterPromise ??= createHighlighter({
    themes: ["vitesse-dark"],
    langs: ["typescript", "javascript", "python", "rust", "json", "bash", "go"],
  });
  return highlighterPromise;
}

async function tokenizeLines(code: string, lang: string) {
  const highlighter = await getHighlighter();
  return highlighter.codeToTokensBase(code, { lang, theme: "vitesse-dark" });
}

// Cuts a line's tokens off at `maxChars`, splitting the token that straddles
// the boundary instead of dropping/keeping it whole — keeps colors intact.
function clipLine(line: ThemedToken[], maxChars: number) {
  let used = 0;
  const clipped: ThemedToken[] = [];
  for (const tok of line) {
    if (used >= maxChars) {
      break;
    }
    const remaining = maxChars - used;
    if (tok.content.length <= remaining) {
      clipped.push(tok);
      used += tok.content.length;
    } else {
      clipped.push({ ...tok, content: tok.content.slice(0, remaining) });
      used = maxChars;
    }
  }
  return {
    tokens: clipped,
    wasClipped:
      used >= maxChars && used < line.reduce((n, t) => n + t.content.length, 0),
  };
}

export default async function SnippetOg({
  title = "auth-middleware.ts",
  lang = "typescript",
  langLabel = "TypeScript",
  code = `export function withAuth(handler: Handler) {
  return async (req: Request) => {
    const token = req.headers.get("authorization");
    if (!token) return new Response("Unauthorized", { status: 401 });

    const session = await verifySession(token);
    return handler(req, session);
  };
}`,
  fileCode = "7F3K9B2Q",
}: {
  title?: string;
  lang?: string;
  langLabel?: string;
  code?: string;
  fileCode?: string;
}) {
  const allLines = await tokenizeLines(code, lang);
  const totalLines = allLines.length;
  const shown = allLines.slice(0, MAX_LINES);
  const linesTruncated = totalLines > MAX_LINES;
  const hiddenLineCount = totalLines - MAX_LINES;

  return (
    <div
      tw="relative flex h-full w-full flex-col justify-between"
      style={{ background: colors.ink, padding: canvas.padding }}
    >
      <BgGlow />

      <div tw="flex items-center justify-between">
        <Wordmark scale={0.58} />
        {title && (
          <span
            style={{
              fontSize: 19,
              color: colors.faded,
              fontFamily: "monospace",
              whiteSpace: "nowrap",
              overflow: "hidden",
              maxWidth: 400,
            }}
          >
            {clipText(title, MAX_TITLE_CHARS)}
          </span>
        )}
      </div>

      <div
        tw="relative flex flex-col"
        style={{
          borderRadius: 18,
          overflow: "hidden",
          border: `1px solid ${colors.panelBorder}`,
          background: "#121110",
        }}
      >
        <div
          tw="flex items-center justify-between"
          style={{
            padding: "14px 22px",
            background: colors.panel,
            borderBottom: `1px solid ${colors.panelBorder}`,
          }}
        >
          <div tw="flex items-center" style={{ gap: 9 }}>
            {dotColors.map((c) => (
              <div
                key={c}
                style={{
                  width: 13,
                  height: 13,
                  borderRadius: 13,
                  background: c,
                }}
              />
            ))}
          </div>
          <div
            style={{
              padding: "5px 14px",
              borderRadius: 999,
              background: colors.primarySoft,
              color: colors.primary,
              fontSize: 16,
              fontWeight: 700,
              letterSpacing: 1,
            }}
          >
            {langLabel}
          </div>
        </div>

        <div tw="flex flex-col" style={{ padding: "26px 30px", gap: 4 }}>
          {shown.map((line, i) => {
            const { tokens, wasClipped } = clipLine(line, MAX_CHARS_PER_LINE);
            const isLastVisible = i === shown.length - 1;
            return (
              <div key={i} tw="flex items-start" style={{ gap: 24 }}>
                <span
                  style={{
                    fontFamily: "monospace",
                    fontSize: 22,
                    color: "#5a5450",
                    width: 28,
                    textAlign: "right",
                    flexShrink: 0,
                  }}
                >
                  {i + 1}
                </span>
                <span
                  style={{
                    fontFamily: "monospace",
                    fontSize: 22,
                    whiteSpace: "pre",
                  }}
                >
                  {tokens.length === 0 ? (
                    <span> </span>
                  ) : (
                    tokens.map((tok, j) => (
                      <span
                        key={j}
                        style={{ color: tok.color ?? colors.paper }}
                      >
                        {tok.content}
                      </span>
                    ))
                  )}
                  {wasClipped && !(isLastVisible && linesTruncated) && (
                    <span style={{ color: "#5a5450" }}>…</span>
                  )}
                </span>
              </div>
            );
          })}
        </div>

        {linesTruncated && (
          <>
            <div
              tw="absolute"
              style={{
                left: 0,
                right: 0,
                bottom: 0,
                height: 72,
                background:
                  "linear-gradient(to bottom, rgba(18,17,16,0), #121110)",
              }}
            />
            <div
              tw="absolute flex items-center"
              style={{
                right: 22,
                bottom: 16,
                padding: "5px 14px",
                borderRadius: 999,
                background: colors.panel,
                border: `1px solid ${colors.panelBorder}`,
                fontFamily: "monospace",
                fontSize: 15,
                fontWeight: 700,
                color: colors.faded,
              }}
            >
              +{hiddenLineCount} more lines
            </div>
          </>
        )}
      </div>

      <div tw="flex items-end justify-between">
        <span style={{ fontSize: 19, color: colors.faded }}>
          Code snippet · Tessera
        </span>
        <CodeChip code={fileCode} big />
      </div>
    </div>
  );
}

export const ogOptions = { width: canvas.width, height: canvas.height };
