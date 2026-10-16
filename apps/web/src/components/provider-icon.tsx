import {
  IconBrandDiscord,
  IconBrandGithub,
  IconKey,
} from "@tabler/icons-react";

/**
 * The provider code is a string from the server, not an enum, so a new provider
 * does not break the frontend; it just gets no dedicated icon. Anything
 * unrecognised gets the key icon so the button is never empty.
 */
export function ProviderIcon({
  provider,
  size = 16,
}: {
  provider: string;
  size?: number;
}) {
  switch (provider) {
    case "github": {
      return <IconBrandGithub size={size} />;
    }
    case "discord": {
      return <IconBrandDiscord size={size} />;
    }
    default: {
      return <IconKey size={size} />;
    }
  }
}
