/**
 * Our own session mark: a filled circle in the status color with a small
 * white right-pointing triangle (a "session" play mark), drawn as a path so
 * no font glyph is involved. Deliberately unlike any Anthropic/Claude artwork.
 */
interface Props {
  color: string;
  size?: number;
  /** Accessible name; without it the mark is decorative. */
  label?: string;
}

export default function SessionMark({ color, size = 14, label }: Props) {
  const a11y = label ? { role: "img", "aria-label": label } : { "aria-hidden": true };
  return (
    <svg className="smark" width={size} height={size} viewBox="0 0 16 16" {...a11y}>
      <circle cx="8" cy="8" r="8" fill={color} />
      <path d="M6.3 4.8 L11.2 8 L6.3 11.2 Z" fill="#fff" />
    </svg>
  );
}
