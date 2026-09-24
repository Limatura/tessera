import {
  IconBrandDiscord,
  IconBrandGithub,
  IconKey,
} from "@tabler/icons-react";

/**
 * provider 代碼是後端給的字串，不是 enum —— 之後多一家時前端不改也不會壞，
 * 只是拿不到專屬圖示。所以認不得的一律用鑰匙，別讓按鈕變成空的。
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
