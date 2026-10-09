import type { components } from "@tessera/api/schema";

type StatsDay = components["schemas"]["StatsDay"];

/** 圖上的一天。`null` 是那天沒有紀錄（伺服器沒在跑），畫成斷點而不是連過去。 */
export interface TrendPoint {
  date: string;
  storedBytes: number | null;
  shares: number | null;
  liveShares: number | null;
  bytesServed: number | null;
}

export type TrendKey = Exclude<keyof TrendPoint, "date">;

const MS_PER_DAY = 86_400_000;

/**
 * 把 server 給的日子攤成連續的每一天。
 *
 * server 只回有紀錄的日子，沒開的那天直接不在清單裡。照原樣丟給圖表會讓前後
 * 兩天緊貼、線直接連過去，看起來像那段時間什麼都沒變，所以中間缺的補 `null`
 * 讓圖斷開。
 *
 * 第一筆之前不補：歷史從這版部署那天才開始記，選 90 天卻只有 3 天資料時，前
 * 面 87 天的空白只會把真資料擠到角落。
 */
export function buildTrendPoints(days: StatsDay[]): {
  points: TrendPoint[];
  hasGaps: boolean;
} {
  const first = days.at(0);
  const last = days.at(-1);
  if (!first || !last) {
    return { hasGaps: false, points: [] };
  }

  const byDate = new Map(days.map((day) => [day.date, day]));
  const points: TrendPoint[] = [];
  const end = dayNumber(last.date);
  for (let current = dayNumber(first.date); current <= end; current += 1) {
    const date = new Date(current * MS_PER_DAY)
      .toISOString()
      .slice(0, "YYYY-MM-DD".length);
    const day = byDate.get(date);
    points.push({
      bytesServed: day?.bytes_served ?? null,
      date,
      liveShares: day?.live_shares ?? null,
      shares: day?.shares ?? null,
      storedBytes: day?.stored_bytes ?? null,
    });
  }

  return { hasGaps: points.length > days.length, points };
}

/** 第一個有值的點到最後一個有值的點，差了多少、橫跨幾天。少於兩個點沒有變化可說。 */
export function trendChange(
  points: TrendPoint[],
  key: TrendKey
): { delta: number; days: number } | null {
  const recorded = points.filter((point) => point[key] !== null);
  const first = recorded.at(0);
  const last = recorded.at(-1);
  if (!first || !last || first === last) {
    return null;
  }
  return {
    days: dayNumber(last.date) - dayNumber(first.date),
    delta: (last[key] ?? 0) - (first[key] ?? 0),
  };
}

/** `YYYY-MM-DD`（UTC）→ 從 epoch 起算第幾天，好逐日往前推、算間隔。 */
function dayNumber(date: string): number {
  return Math.round(Date.parse(`${date}T00:00:00Z`) / MS_PER_DAY);
}
