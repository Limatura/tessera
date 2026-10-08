import { createFileRoute } from "@tanstack/react-router";

import { InterfaceThemeField } from "@/components/interface-theme-field";
import * as m from "@/paraglide/messages";

import { AppearanceSettingsForm } from "./-components/appearance-settings-form";
import { SettingsCard, SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/theme")({
  component: ThemeSettingsRoute,
});

/**
 * 兩件不同的事放同一頁：上面是「這個後台在我自己的瀏覽器裡長怎樣」（個人偏好，
 * 不用存檔、不影響別人），下面才是站台設定。分成兩張卡，免得以為上面那個也要按儲存。
 */
function ThemeSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_theme_description()}
      title={m.settings_theme_title()}
    >
      <AppearanceSettingsForm />
      <SettingsCard
        description={m.settings_theme_personal_description()}
        title={m.settings_theme_personal_title()}
      >
        <InterfaceThemeField />
      </SettingsCard>
    </SettingsPage>
  );
}
