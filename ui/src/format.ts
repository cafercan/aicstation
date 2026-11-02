export function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

export function fmtCost(usd: number): string {
  return `$${usd.toFixed(2)}`;
}

export function fmtElapsed(ms: number): string {
  const min = Math.floor(Math.max(0, ms) / 60_000);
  if (min < 1) return "şimdi";
  if (min < 60) return `${min} dk`;
  const h = Math.floor(min / 60);
  if (h < 24) return min % 60 ? `${h} sa ${min % 60} dk` : `${h} sa`;
  return `${Math.floor(h / 24)} gün`;
}

export function statusLabel(status: string): string {
  switch (status) {
    case "busy": return "çalışıyor";
    case "waiting": return "onay bekliyor";
    case "idle": return "boşta";
    default: return status || "?";
  }
}

/** Last component of a Windows or POSIX folder path ("" if none). */
export function folderName(cwd: string): string {
  return cwd.split(/[\\/]/).filter((s) => s !== "").pop() ?? "";
}

export function statusClass(status: string): "busy" | "waiting" | "idle" {
  return status === "busy" || status === "waiting" ? status : "idle";
}
