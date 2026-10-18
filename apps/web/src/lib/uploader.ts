import { createUploader } from "@tessera/upload";

/**
 * Shares on the instance this page was served from, as the signed-in session
 * (if any). The web app is embedded in the server binary, so its own origin is
 * the API's.
 */
export const uploader = createUploader({
  baseUrl: window.location.origin,
  credentials: "include",
});
