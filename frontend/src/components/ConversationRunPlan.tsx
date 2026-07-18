import {
  CheckCircle2,
  Circle,
  LoaderCircle,
  MinusCircle,
  XCircle
} from "lucide-react";
import { useI18n, type TranslationKey } from "../i18n";
import type { ConversationExecutionPlan, ConversationExecutionPlanStep } from "../types/domain";
import {
  executionPlanDimensionKey,
  executionPlanScopeKey,
  executionPlanStepKey
} from "../pages/homeRules";

export function ConversationRunPlan({ plan }: { plan: ConversationExecutionPlan }) {
  const { t } = useI18n();
  const countable = plan.steps.filter((step) => step.status !== "skipped");
  const completed = countable.filter((step) => step.status === "completed").length;

  return (
    <section className="conversation-run-plan" aria-label={t("home.planTitle")}>
      <header>
        <div>
          <strong>{t("home.planTitle")}</strong>
          <span>{t("home.planProgress", { completed, total: countable.length })}</span>
        </div>
        <p>
          <b>{t("home.planScopeLabel")}</b>
          {t(executionPlanScopeKey(plan.scope))}
        </p>
      </header>
      <div className="conversation-run-plan-template">
        <b>{t("home.planTemplate")}</b>
        <span>
          {plan.dimensions
            .map((dimension) => t(executionPlanDimensionKey(dimension)))
            .join(" · ")}
        </span>
      </div>
      <ol>
        {plan.steps.map((step) => (
          <li key={step.id} data-status={step.status}>
            <PlanStepIcon step={step} label={t(planStepStatusKey(step.status))} />
            <span>{t(executionPlanStepKey(step.id))}</span>
          </li>
        ))}
      </ol>
    </section>
  );
}

function PlanStepIcon({ step, label }: { step: ConversationExecutionPlanStep; label: string }) {
  const properties = { size: 16, "aria-label": label };
  if (step.status === "completed") return <CheckCircle2 {...properties} />;
  if (step.status === "running") return <LoaderCircle {...properties} className="plan-step-spinner" />;
  if (step.status === "skipped") return <MinusCircle {...properties} />;
  if (step.status === "failed") return <XCircle {...properties} />;
  return <Circle {...properties} />;
}

function planStepStatusKey(status: string): TranslationKey {
  if (status === "completed") return "home.planStatusCompleted";
  if (status === "running") return "home.planStatusRunning";
  if (status === "skipped") return "home.planStatusSkipped";
  if (status === "failed") return "home.planStatusFailed";
  return "home.planStatusPending";
}
