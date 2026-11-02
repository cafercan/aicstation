/**
 * Our own session mark: a filled circle in the status color with a white
 * "✻" (U+273B). Deliberately not any official Anthropic/Claude artwork.
 */
interface Props {
  color: string;
  size?: number;
  /** Accessible name; without it the mark is decorative. */
  label?: string;
}

export default function ClaudeMark({ color, size = 14, label }: Props) {
  const a11y = label ? { role: "img", "aria-label": label } : { "aria-hidden": true };
  return (
    <svg className="cmark" width={size} height={size} viewBox="0 0 16 16" {...a11y}>
      <circle cx="8" cy="8" r="8" fill={color} />
      <text x="8" y="8.6" textAnchor="middle" dominantBaseline="middle" fill="#fff"
        fontSize="11" fontWeight="700">{"✻"}</text>
    </svg>
  );
}
