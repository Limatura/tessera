// Ticket icon (logo.svg). The torn-off stub on the right follows --primary, so
// it changes with the accent.
// No backing plate: the ticket and its text/notch use foreground/background, so
// light and dark swap automatically.
export function Logo({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 512 512"
      xmlns="http://www.w3.org/2000/svg"
    >
      <title>Tessera</title>
      <defs>
        <clipPath id="logo-ticket">
          <path d="M92 160 H420 A28 28 0 0 1 448 188 V222 A34 34 0 0 0 448 290 V324 A28 28 0 0 1 420 352 H92 A28 28 0 0 1 64 324 V290 A34 34 0 0 0 64 222 V188 A28 28 0 0 1 92 160 Z" />
        </clipPath>
      </defs>
      <g transform="rotate(-12 256 256)">
        <g clipPath="url(#logo-ticket)">
          <rect
            className="fill-foreground"
            height="512"
            width="340"
            x="0"
            y="0"
          />
          <rect
            className="fill-primary"
            height="512"
            width="172"
            x="340"
            y="0"
          />
        </g>
        <g className="fill-background">
          <rect height="22" rx="8" width="16" x="332" y="175" />
          <rect height="22" rx="8" width="16" x="332" y="214" />
          <rect height="22" rx="8" width="16" x="332" y="253" />
          <rect height="22" rx="8" width="16" x="332" y="292" />
          <rect height="22" rx="8" width="16" x="332" y="331" />
        </g>
        <g className="fill-background">
          <path d="M118 304 L156 208 H186 L148 304 Z" />
          <path d="M170 304 L208 208 H238 L200 304 Z" />
          <path d="M222 304 L260 208 H290 L252 304 Z" />
        </g>
      </g>
    </svg>
  );
}
