import { AnimatedThemeToggler } from "@tessera/ui/components/animated-theme-toggler";
import { buttonVariants } from "@tessera/ui/components/button";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";

import { getLocale, setLocale } from "@/lib/locale";
import { m } from "@/paraglide/messages.js";
import { locales } from "@/paraglide/runtime.js";

export default function Header() {
  const locale = getLocale();

  return (
    <nav className="fixed w-full z-50">
      <div className="flex justify-end gap-2 p-4">
        <Select
          value={locale}
          onValueChange={(value) => setLocale(value as typeof locale)}
        >
          <SelectTrigger className="w-24">
            <SelectValue placeholder={m.common_language()} />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {locales.map((availableLocale) => (
                <SelectItem key={availableLocale} value={availableLocale}>
                  {availableLocale.toUpperCase()}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
        <AnimatedThemeToggler
          className={buttonVariants({ size: "icon", variant: "outline" })}
        />
      </div>
    </nav>
  );
}
