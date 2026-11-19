import { useT } from "../i18n";
import { CONV_FONT_MAX, CONV_FONT_MIN, setConvFontSize, useConvFontSize } from "../conversation/convAppearance";
import { Slider } from "./TerminalAppearance";

/** Settings → Conversation appearance: the Conversation tab's font size. */
export default function ConversationAppearance() {
  const t = useT();
  const size = useConvFontSize();
  return (
    <fieldset>
      <legend>{t("conv.lookTitle")}</legend>
      <Slider label={t("conv.fontSize")} value={size} min={CONV_FONT_MIN} max={CONV_FONT_MAX} step={1} unit=" px"
        onChange={setConvFontSize} />
    </fieldset>
  );
}
