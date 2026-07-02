import type {
  DecisionDeltaLeg,
  DecisionDeltaSnapshot,
  DecisionDeltaTimelineItem
} from "../types/domain";

export type DecisionComparisonKind =
  | "asset_vs_cash"
  | "cash_vs_holding"
  | "cash_vs_hypothetical"
  | "manual";

export interface VisibleDecisionDeltaSummary {
  visibleDecisionsCount: number;
  quantifiableDecisionsCount: number;
  positiveDeltaCount: number;
  negativeDeltaCount: number;
  staleCount: number;
  sumDeltaValue: number;
  lastRefreshedAt: string | null;
}

export function summarizeVisibleDecisionDeltas(
  items: DecisionDeltaTimelineItem[]
): VisibleDecisionDeltaSummary {
  return items.reduce<VisibleDecisionDeltaSummary>(
    (summary, item) => {
      const snapshot = item.latest_snapshot;
      summary.visibleDecisionsCount += 1;
      if (item.quantifiable) {
        summary.quantifiableDecisionsCount += 1;
      }
      if (!snapshot) {
        return summary;
      }

      summary.sumDeltaValue += snapshot.delta_value;
      if (snapshot.delta_value > 0) {
        summary.positiveDeltaCount += 1;
      }
      if (snapshot.delta_value < 0) {
        summary.negativeDeltaCount += 1;
      }
      if (isDecisionDeltaStale(snapshot)) {
        summary.staleCount += 1;
      }
      if (!summary.lastRefreshedAt || snapshot.created_at > summary.lastRefreshedAt) {
        summary.lastRefreshedAt = snapshot.created_at;
      }
      return summary;
    },
    {
      visibleDecisionsCount: 0,
      quantifiableDecisionsCount: 0,
      positiveDeltaCount: 0,
      negativeDeltaCount: 0,
      staleCount: 0,
      sumDeltaValue: 0,
      lastRefreshedAt: null
    }
  );
}

export function formatDecisionDeltaMoney(value: number, currency = "CNY") {
  const formatted = new Intl.NumberFormat("en-US", {
    style: "currency",
    currency,
    maximumFractionDigits: 2
  }).format(Math.abs(value));

  if (value > 0) {
    return `+${formatted}`;
  }
  if (value < 0) {
    return `-${formatted}`;
  }
  return formatted;
}

export function formatDecisionDeltaPercent(value?: number | null) {
  if (value == null || !Number.isFinite(value)) {
    return "n/a";
  }
  const sign = value > 0 ? "+" : "";
  return `${sign}${(value * 100).toFixed(1)}%`;
}

export function comparisonKindForAction(action: string): DecisionComparisonKind {
  switch (action.trim().toLowerCase()) {
    case "buy":
    case "add":
      return "asset_vs_cash";
    case "sell":
    case "trim":
      return "cash_vs_holding";
    case "watch":
    case "skip":
      return "cash_vs_hypothetical";
    default:
      return "manual";
  }
}

export function singleForkLegs(legs: DecisionDeltaLeg[]) {
  return {
    actual: legs.find((leg) => leg.leg_kind === "actual") ?? null,
    baseline: legs.find((leg) => leg.leg_kind === "baseline") ?? null
  };
}

export function isDecisionDeltaStale(snapshot?: DecisionDeltaSnapshot | null) {
  return Boolean(snapshot?.price_stale || snapshot?.fx_stale);
}

export function decisionDeltaTone(snapshot?: Pick<DecisionDeltaSnapshot, "delta_value"> | null) {
  if (!snapshot) {
    return "neutral";
  }
  if (snapshot.delta_value > 0) {
    return "positive";
  }
  if (snapshot.delta_value < 0) {
    return "warning";
  }
  return "neutral";
}
