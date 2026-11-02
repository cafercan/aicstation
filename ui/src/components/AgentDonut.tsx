import { donutSegments } from "../format";

/** r chosen so the circumference is 100 and dash lengths are percentages. */
const R = 15.9155;

export default function AgentDonut({ running, finished }: { running: number; finished: number }) {
  const total = Math.max(0, running) + Math.max(0, finished);
  return (
    <svg className="donut" width="74" height="74" viewBox="0 0 36 36"
      role="img" aria-label={`${total} ajan: ${running} çalışıyor, ${finished} bitti`}>
      <circle cx="18" cy="18" r={R} fill="none" stroke="#252b36" strokeWidth="4" />
      {donutSegments(running, finished).map((s) => (
        <circle key={s.kind} data-seg={s.kind} cx="18" cy="18" r={R} fill="none" stroke={s.color}
          strokeWidth="4" strokeDasharray={`${+s.length.toFixed(3)} ${+(100 - s.length).toFixed(3)}`}
          strokeDashoffset={+s.offset.toFixed(3)} />
      ))}
      <text x="18" y="18" textAnchor="middle" fill={total > 0 ? "#fff" : "#7d8696"}
        fontSize="9" fontWeight="700">{total}</text>
      <text x="18" y="24.5" textAnchor="middle" fill="#7d8696" fontSize="4.2">ajan</text>
    </svg>
  );
}
