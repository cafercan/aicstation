import { useT } from "../i18n";

/** Product name, version, tagline and the non-affiliation note (bottom of the settings page). */
export default function About() {
  const t = useT();
  return (
    <footer className="about">
      <div className="about-name">AI C-Station</div>
      <div className="about-ver">{t("about.version", { v: __APP_VERSION__ })}</div>
      <div className="about-tag">{t("about.tagline")}</div>
      <div className="about-note">{t("about.disclaimer")}</div>
    </footer>
  );
}
