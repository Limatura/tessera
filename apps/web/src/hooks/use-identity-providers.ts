import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

export type Provider = components["schemas"]["ProviderResponse"];
export type Identity = components["schemas"]["IdentityResponse"];

const PROVIDERS_KEY = ["auth", "providers"] as const;
const IDENTITIES_KEY = ["auth", "identities"] as const;

/**
 * 三家都會回，關掉的也會 —— 登入頁要先問過才知道要畫幾顆按鈕。
 * 帳號設定則是連沒開的那家都不提，所以兩邊都要自己濾 `enabled`。
 *
 * 帳號功能整個關掉時這隻回 404，`error` 會是 ErrorBody；那種站台本來就沒有
 * 登入頁可言，所以錯誤一律當成「沒有第三方登入」處理（見 providerList）。
 */
export const providersQueryOptions = queryOptions({
  queryFn: async (): Promise<Provider[]> => {
    const { data, error } = await api.GET("/api/v1/auth/providers");
    if (error) {
      throw error;
    }
    return data;
  },
  queryKey: PROVIDERS_KEY,
  retry: false,
  // 這是站台設定，不是使用者資料，一次載入期間不會變。
  staleTime: 1000 * 60 * 5,
});

export function useProviders() {
  return useQuery(providersQueryOptions);
}

/** 只有真的設定好的才畫得出按鈕。查詢失敗就是空清單，密碼登入照常。 */
export function useEnabledProviders(): Provider[] {
  const { data } = useProviders();
  return data?.filter((provider) => provider.enabled) ?? [];
}

export function useIdentities() {
  return useQuery({
    queryFn: async (): Promise<Identity[]> => {
      const { data, error } = await api.GET("/api/v1/auth/oidc/identities");
      if (error) {
        throw error;
      }
      return data;
    },
    queryKey: IDENTITIES_KEY,
  });
}

/**
 * 解除綁定。409 代表這是最後一個登入方式 —— 後端自己算過（它知道有沒有密碼，
 * 前端不知道），message 是寫給人看的，直接顯示。
 */
export function useUnlinkIdentity() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (provider: string) => {
      const { error } = await api.DELETE(
        "/api/v1/auth/oidc/identities/{provider}",
        { params: { path: { provider } } }
      );
      if (error) {
        throw error;
      }
    },
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: IDENTITIES_KEY }),
  });
}
