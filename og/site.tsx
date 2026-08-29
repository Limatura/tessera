import { colors, Wordmark, BgGlow, canvas } from "./tokens";

function Pill({ label, muted = false }: { label: string; muted?: boolean }) {
  return (
    <div
      style={{
        padding: "9px 18px",
        borderRadius: 999,
        border: `1px solid ${muted ? colors.panelBorder : colors.primarySoftBorder}`,
        background: muted ? colors.panel : colors.primarySoft,
        fontSize: 18,
        fontWeight: 700,
        letterSpacing: 1.5,
        color: muted ? colors.faded : colors.primary,
      }}
    >
      {label}
    </div>
  );
}

export default function SiteOg() {
  return (
    <div
      tw="relative flex h-full w-full flex-col justify-between"
      style={{ background: colors.ink, padding: canvas.padding }}
    >
      <BgGlow />

      <Wordmark scale={1.35} />

      <div tw="flex flex-col" style={{ maxWidth: 880, gap: 20 }}>
        <h1
          style={{
            margin: 0,
            fontSize: 76,
            fontWeight: 800,
            lineHeight: 1.02,
            letterSpacing: -2,
            color: colors.paper,
          }}
        >
          Anonymous file courier.
        </h1>
        <p
          style={{
            margin: 0,
            fontSize: 30,
            lineHeight: 1.4,
            color: colors.faded,
            maxWidth: 760,
          }}
        >
          Upload a file, a snippet, or a link. Hand out a retrieval code. It
          self-destructs when it expires or runs out of uses.
        </p>
      </div>

      <div tw="flex items-center" style={{ gap: 14 }}>
        <Pill label="FILES" muted />
        <Pill label="CODE" muted />
        <Pill label="LINKS" muted />
        <Pill label="NO SIGNUP" />
      </div>
    </div>
  );
}

export const ogOptions = { width: canvas.width, height: canvas.height };
