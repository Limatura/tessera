import template from "./tessera-upload.sh?raw";

/**
 * `tessera-upload`, the shell script, with this instance and key written in.
 *
 * The script is `tessera-upload.sh` beside this file; only its two
 * placeholders change. Both land inside single quotes, so a quote in either
 * value is closed, escaped and reopened rather than ending the string.
 */
export function shellScript({
  origin,
  secret,
}: {
  origin: string;
  secret: string;
}): string {
  return template
    .replace(URL_PLACEHOLDER, singleQuoted(origin))
    .replace(TOKEN_PLACEHOLDER, singleQuoted(secret));
}

export const SHELL_SCRIPT_FILE_NAME = "tessera-upload";

function singleQuoted(value: string): string {
  return value.replaceAll("'", String.raw`'\''`);
}

const URL_PLACEHOLDER = "__TESSERA_URL__";
const TOKEN_PLACEHOLDER = "__TESSERA_TOKEN__";
