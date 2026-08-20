/**
 * Hand-drawn line icons rather than an icon library.
 *
 * This app needs about a dozen icons, while a library like lucide adds hundreds of kilobytes
 * and one more dependency to keep up to date. They all use a 24 grid, the same stroke width,
 * and `currentColor` so they inherit the colour of whatever button they sit in.
 */
interface Props {
  size?: number;
  className?: string;
}

function Svg({ size = 17, className, children }: Props & { children: React.ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export const ArrowLeft = (p: Props) => (
  <Svg {...p}>
    <path d="M19 12H5" />
    <path d="m12 19-7-7 7-7" />
  </Svg>
);

export const Download = (p: Props) => (
  <Svg {...p}>
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <path d="m7 10 5 5 5-5" />
    <path d="M12 15V3" />
  </Svg>
);

/** Captions — used for SRT export. */
export const Captions = (p: Props) => (
  <Svg {...p}>
    <rect x="3" y="5" width="18" height="14" rx="2" />
    <path d="M7 11h3" />
    <path d="M14 11h3" />
    <path d="M7 15h4" />
    <path d="M15 15h2" />
  </Svg>
);

export const Mic = (p: Props) => (
  <Svg {...p}>
    <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z" />
    <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
    <path d="M12 19v3" />
  </Svg>
);

export const Trash = (p: Props) => (
  <Svg {...p}>
    <path d="M3 6h18" />
    <path d="M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2" />
    <path d="m19 6-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
  </Svg>
);

/** The app owner's own voice — toggles the microphone track in and out of view. */
export const User = (p: Props) => (
  <Svg {...p}>
    <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
    <circle cx="12" cy="7" r="4" />
  </Svg>
);

export const Clock = (p: Props) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </Svg>
);

export const Sliders = (p: Props) => (
  <Svg {...p}>
    <path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3" />
    <path d="M1 14h6M9 8h6M17 16h6" />
  </Svg>
);

export const Pin = (p: Props) => (
  <Svg {...p}>
    <path d="M12 17v5" />
    <path d="M9 10.8a2 2 0 0 1-1.1 1.8l-1.8.9A2 2 0 0 0 5 15.2V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.8a2 2 0 0 0-1.1-1.8l-1.8-.9A2 2 0 0 1 15 10.8V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z" />
  </Svg>
);

export const Search = (p: Props) => (
  <Svg {...p}>
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </Svg>
);

export const TextSmaller = (p: Props) => (
  <Svg {...p}>
    <path d="m3 18 5-12 5 12" />
    <path d="M4.8 14h6.4" />
    <path d="M16 11h6" />
  </Svg>
);

export const TextLarger = (p: Props) => (
  <Svg {...p}>
    <path d="m3 18 5-12 5 12" />
    <path d="M4.8 14h6.4" />
    <path d="M16 11h6" />
    <path d="M19 8v6" />
  </Svg>
);
