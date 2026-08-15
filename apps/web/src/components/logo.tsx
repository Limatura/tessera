export function Logo({ className }: { className?: string }) {
  return (
    <svg
      className={`logo-root ${className ?? ""}`}
      fill="none"
      viewBox="0 0 300 35"
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <linearGradient
          gradientUnits="userSpaceOnUse"
          id="logo-polish"
          x1="0"
          x2="300"
          y1="0"
          y2="0"
        >
          <stop
            className="[stop-color:#b8b8b8] dark:[stop-color:#4a4a4a]"
            offset="0%"
          />
          <stop
            className="[stop-color:#6a6a6a] dark:[stop-color:#8a8a8a]"
            offset="45%"
          />
          <stop
            className="[stop-color:#0a0a0a] dark:[stop-color:#ffffff]"
            offset="100%"
          />
        </linearGradient>
        <linearGradient id="logo-shine" x1="0" x2="1" y1="0" y2="0">
          <stop offset="0%" stopColor="#ffffff" stopOpacity="0" />
          <stop offset="50%" stopColor="#ffffff" stopOpacity="1" />
          <stop offset="100%" stopColor="#ffffff" stopOpacity="0" />
        </linearGradient>
        <mask
          height="150"
          id="logo-shape"
          maskContentUnits="userSpaceOnUse"
          maskUnits="userSpaceOnUse"
          width="400"
          x="-50"
          y="-50"
        >
          <g fill="white">
            <g transform="translate(0,-13.6) scale(0.972)">
              <path d="M2 50 L5 50 L13 14 L10 14 Z" />
              <path d="M11 50 L15 50 L23 14 L19 14 Z" />
              <path d="M20 50 L25 50 L33 14 L28 14 Z" />
              <path d="M29 50 L35 50 L43 14 L37 14 Z" />
              <path d="M38 50 L54 50 L62 14 L46 14 Z" />
            </g>
            <text
              fontFamily="var(--font-sans, sans-serif)"
              fontSize="46"
              fontWeight="700"
              x="75"
              y="34"
            >
              Tessera
            </text>
          </g>
        </mask>
      </defs>
      <g fill="url(#logo-polish)">
        <g transform="translate(0,-13.6) scale(0.972)">
          <path d="M2 50 L5 50 L13 14 L10 14 Z" />
          <path d="M11 50 L15 50 L23 14 L19 14 Z" />
          <path d="M20 50 L25 50 L33 14 L28 14 Z" />
          <path d="M29 50 L35 50 L43 14 L37 14 Z" />
          <path d="M38 50 L54 50 L62 14 L46 14 Z" />
        </g>
        <text
          fontFamily="var(--font-sans, sans-serif)"
          fontSize="46"
          fontWeight="700"
          x="75"
          y="34"
        >
          Tessera
        </text>
      </g>
      <rect
        className="logo-shine"
        fill="url(#logo-shine)"
        height="55"
        mask="url(#logo-shape)"
        width="140"
        y="-10"
      />
      <style>{`
        .logo-shine {
          x: -160px;
          animation: logo-shine-mount 1s ease-out 1;
        }
        .logo-root:hover .logo-shine {
          animation: logo-shine-hover 1s ease-out 1;
        }
        @keyframes logo-shine-mount {
          from { x: -160px; }
          to { x: 300px; }
        }
        @keyframes logo-shine-hover {
          from { x: -160px; }
          to { x: 300px; }
        }
      `}</style>
    </svg>
  );
}
