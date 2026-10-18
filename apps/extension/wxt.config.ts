import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "wxt";

// See https://wxt.dev/api/config.html
export default defineConfig({
  // MV3 for Firefox too. WXT would build MV2 there, which has no
  // `optional_host_permissions`, and the instance's address is only known once
  // the user types it, so its permission has to be asked for at run time.
  manifestVersion: 3,
  manifest: ({ browser }) => ({
    name: "__MSG_extName__",
    description: "__MSG_extDescription__",
    default_locale: "zh_TW",
    action: { default_title: "__MSG_extName__" },
    permissions: [
      // The right-click menu.
      "contextMenus",
      // Settings, the sign-in, and the progress of what is uploading.
      "storage",
      // The page right-clicked on, for as long as that click lasts: what
      // reading its selection or an image it shows needs, and nothing more.
      "activeTab",
      "scripting",
      "notifications",
      "clipboardWrite",
      // Chrome's service worker has no document to copy from; Firefox's
      // background page does.
      ...(browser === "firefox" ? [] : ["offscreen"]),
    ],
    // Asked for one origin at a time: the instance when it is set, and every
    // site only if the user turns on fetching media from anywhere.
    optional_host_permissions: ["http://*/*", "https://*/*"],
    content_security_policy: {
      // WXT's MV3 default, written out because Firefox's own default adds
      // `upgrade-insecure-requests`, which would turn every request to an
      // instance on plain HTTP (a LAN, Tailscale) into HTTPS and break it.
      extension_pages:
        "script-src 'self' 'wasm-unsafe-eval'; object-src 'self';",
    },
    ...(browser === "firefox"
      ? {
          browser_specific_settings: {
            gecko: {
              id: "extension@tessera",
              // `optional_host_permissions` and `world: "MAIN"` in executeScript.
              strict_min_version: "128.0",
            },
          },
        }
      : {}),
  }),
  modules: ["@wxt-dev/module-react", "@wxt-dev/i18n/module"],
  vite: () => ({
    plugins: [tailwindcss()],
  }),
});
