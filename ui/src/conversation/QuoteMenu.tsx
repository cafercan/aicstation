import ContextMenu, { type MenuItem } from "../components/ContextMenu";
import { useT } from "../i18n";

/**
 * Right-click menu over selected conversation text: Quote (only where there
 * is a message box) and Copy. See `ContextMenu` for its keys and closing.
 */
export default function QuoteMenu({ x, y, onQuote, onCopy, onClose }: {
  x: number; y: number; onQuote?: () => void; onCopy: () => void; onClose: () => void;
}) {
  const t = useT();
  const items: MenuItem[] = [
    ...(onQuote ? [{ label: t("conv.quote"), onSelect: onQuote }] : []),
    { label: t("conv.copy"), onSelect: onCopy },
  ];
  return <ContextMenu x={x} y={y} items={items} onClose={onClose} />;
}
