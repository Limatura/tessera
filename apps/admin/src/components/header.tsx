import { Link } from "@tanstack/react-router";
import { useI18n } from "@tessera/i18n";

import { ModeToggle } from "./mode-toggle";

export default function Header() {
  const { t, locale, setLocale, availableLocales } = useI18n();
  const links = [{ label: t("header.home"), to: "/" }] as const;

  return (
    <div>
      <div className="flex flex-row items-center justify-between px-2 py-1">
        <nav className="flex gap-4 text-lg">
          {links.map(({ to, label }) => (
            <Link key={to} to={to}>
              {label}
            </Link>
          ))}
        </nav>
        <div className="flex items-center gap-2">
          <label className="text-sm" htmlFor="locale-switcher">
            {t("common.language")}
          </label>
          <select
            id="locale-switcher"
            className="rounded border px-2 py-1 text-sm"
            value={locale}
            onChange={(event) => setLocale(event.target.value as typeof locale)}
          >
            {availableLocales.map((availableLocale) => (
              <option key={availableLocale} value={availableLocale}>
                {availableLocale.toUpperCase()}
              </option>
            ))}
          </select>
          <ModeToggle />
        </div>
      </div>
      <hr />
    </div>
  );
}
