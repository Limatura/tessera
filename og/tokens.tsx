// Design tokens derived from tessera's real theme
// (packages/ui/src/styles/globals.css, oklch -> hex approximation)
export const colors = {
  // dark mode --background (oklch(0.147 0.004 49.3))
  ink: "#1c1917",
  // one step up, for cards
  panel: "#241f1c",
  panelBorder: "rgba(250,250,249,0.09)",
  // dark mode --foreground (oklch(0.986 0.002 67.8))
  paper: "#faf9f7",
  // muted foreground, warm stone-400
  faded: "#a39a92",
  // light-mode --primary (oklch(0.553 0.195 38.402))
  primary: "#c2410c",
  // dark-mode --primary shade (oklch(0.47 0.157 37.304))
  primaryDark: "#7c2d12",
  primarySoft: "rgba(194,65,12,0.16)",
  primarySoftBorder: "rgba(194,65,12,0.45)",
};

export const font = {
  family: "Inter",
};

export const canvas = { width: 1200, height: 630, padding: 64 };

// Character-based clipping — one rule everywhere text has a bounded box,
// instead of relying on CSS text-overflow (unreliable across takumi's engine
// and unusable once a line is split into differently-colored token spans).
export function clipText(text: string, maxChars: number) {
  if (text.length <= maxChars) {
    return text;
  }
  return `${text.slice(0, maxChars)}…`;
}

export const iconBadge = { box: 88, radius: 20, border: 2, glyphRatio: 0.52 };

export function IconBadge({
  icon: Icon,
  size = iconBadge.box,
}: {
  icon: (props: {
    size: number;
    color: string;
    strokeWidth: number;
  }) => JSX.Element;
  size?: number;
}) {
  return (
    <div
      tw="flex items-center justify-center"
      style={{
        width: size,
        height: size,
        borderRadius: iconBadge.radius,
        background: colors.panel,
        border: `${iconBadge.border}px solid ${colors.primary}`,
      }}
    >
      <Icon
        size={size * iconBadge.glyphRatio}
        color={colors.primary}
        strokeWidth={1.75}
      />
    </div>
  );
}

export function Wordmark({ scale = 1 }: { scale?: number }) {
  const bars = [8, 8, 8, 8, 15];
  return (
    <div tw="flex items-center" style={{ gap: 5 * scale }}>
      <div
        tw="flex items-end"
        style={{ gap: 4 * scale, transform: `skewX(-16deg) scale(${scale})` }}
      >
        {bars.map((w, i) => (
          <div
            key={i}
            style={{
              width: w,
              height: 34,
              background: i === bars.length - 1 ? colors.primary : colors.paper,
              opacity: i === bars.length - 1 ? 1 : 0.92,
            }}
          />
        ))}
      </div>
      <span
        style={{
          fontSize: 30 * scale,
          fontWeight: 800,
          letterSpacing: -0.5,
          color: colors.paper,
        }}
      >
        Tessera
      </span>
    </div>
  );
}

export function CodeChip({
  code,
  big = false,
}: {
  code: string;
  big?: boolean;
}) {
  return (
    <div
      tw="flex items-center"
      style={{
        gap: 10,
        padding: big ? "14px 22px" : "8px 16px",
        border: `2px dashed ${colors.primarySoftBorder}`,
        borderRadius: 12,
        background: colors.primarySoft,
      }}
    >
      <span
        style={{
          fontFamily: "monospace",
          fontSize: big ? 34 : 20,
          fontWeight: 700,
          letterSpacing: 3,
          color: colors.paper,
        }}
      >
        {code}
      </span>
    </div>
  );
}

export function BgGlow() {
  return (
    <div
      tw="absolute"
      style={{
        top: -260,
        right: -220,
        width: 760,
        height: 760,
        borderRadius: 760,
        background: `radial-gradient(circle, ${colors.primaryDark} 0%, rgba(124,45,18,0) 70%)`,
        opacity: 0.5,
      }}
    />
  );
}
