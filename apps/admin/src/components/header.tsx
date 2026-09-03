import { Link } from "@tanstack/react-router";

import { getLocale, setLocale } from "@/lib/locale";
import { m } from "@/paraglide/messages.js";
import { locales } from "@/paraglide/runtime.js";

import { ModeToggle } from "./mode-toggle";

export function Header() {
  const locale = getLocale();
  const links = [{ label: m.header_home(), to: "/" }] as const;

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
            {m.common_language()}
          </label>
          <select
            id="locale-switcher"
            className="rounded border px-2 py-1 text-sm"
            value={locale}
            onChange={(event) => setLocale(event.target.value as typeof locale)}
          >
            {locales.map((availableLocale) => (
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
