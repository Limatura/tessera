import { queryOptions, useQuery } from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

type PublicShare = components["schemas"]["PublicShareResponse"];
type Ticket = components["schemas"]["TicketResponse"];

/** 不會花費取回次數——對預先取得、連結預覽、頁面載入都很安全 */
export function shareQueryOptions(code: string, password?: string) {
  return queryOptions({
    queryFn: async (): Promise<PublicShare> => {
      const { data, error, response } = await api.GET("/api/v1/shares/{code}", {
        params: {
          header: password ? { "X-Share-Password": password } : undefined,
          path: { code },
        },
      });
      if (error) {
        throw Object.assign(new Error(response.statusText), {
          status: response.status,
        });
      }
      return data;
    },
    queryKey: ["share", code, password ?? null],
    retry: false,
  });
}

export function useShare(code: string, password?: string) {
  return useQuery(shareQueryOptions(code, password));
}

/**
 * 花費一次取回次數，並快取票券供本次拜訪剩餘時間使用——票券本身涵蓋傳輸所需的每個請求
 * 所以不能靜默重新取得（視窗焦點重新取得會浪費第二次使用機會）
 *
 * 刻意不給 `queryOptions`:它不能被 route loader 預抓，預抓等於替訪客花掉一次。
 */
export function useShareTicket(
  code: string,
  password: string | undefined,
  enabled: boolean
) {
  return useQuery({
    enabled,
    gcTime: Number.POSITIVE_INFINITY,
    queryFn: async (): Promise<Ticket> => {
      const { data, error, response } = await api.POST(
        "/api/v1/shares/{code}/unlock",
        {
          params: {
            header: password ? { "X-Share-Password": password } : undefined,
            path: { code },
          },
        }
      );
      if (error) {
        throw Object.assign(new Error(response.statusText), {
          status: response.status,
        });
      }
      return data;
    },
    queryKey: ["share-ticket", code, password ?? null],
    refetchOnMount: false,
    refetchOnReconnect: false,
    refetchOnWindowFocus: false,
    retry: false,
    staleTime: Number.POSITIVE_INFINITY,
  });
}
