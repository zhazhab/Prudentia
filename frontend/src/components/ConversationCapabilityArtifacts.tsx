import { Bot, BookOpenCheck } from "lucide-react";
import { useI18n, type TranslationKey } from "../i18n";
import { conversationCapabilityArtifacts } from "../pages/homeRules";
import {
  conversationCapabilityPayloadView,
  safeHttpUrl
} from "./conversationCapabilityRules";
import type {
  AgentExecutionTrace,
  ConversationCapabilityArtifact,
  ConversationCapabilityEvidence,
  ConversationCapabilityEvidenceAssessment,
} from "../types/domain";

export function ConversationCapabilityArtifacts({ artifacts }: { artifacts: unknown[] }) {
  const { t } = useI18n();
  const capabilities = conversationCapabilityArtifacts(artifacts);
  if (!capabilities.length) return null;

  return (
    <section className="capability-artifact-stack" aria-label={t("home.capabilityArtifacts")}>
      {capabilities.map((artifact, index) => (
        <details className={`capability-artifact ${artifact.capability_kind} ${artifact.status}`} open={index === 0 ? true : undefined} key={artifact.call_id}>
          <summary>
            <span className="capability-artifact-icon">
              {artifact.capability_kind === "agent" ? <Bot size={17} /> : <BookOpenCheck size={17} />}
            </span>
            <span>
              <strong>{capabilityName(artifact, t)}</strong>
              <small>{capabilityMeta(artifact, t)}</small>
            </span>
          </summary>
          <CapabilityPayload artifact={artifact} />
        </details>
      ))}
    </section>
  );
}

function CapabilityPayload({ artifact }: { artifact: ConversationCapabilityArtifact }) {
  const { t } = useI18n();
  if (artifact.status === "failed") {
    return (
      <div className="capability-artifact-body" role="alert">
        <p className="capability-failure">
          {t(capabilityFailureKey(artifact.error_code))}
        </p>
      </div>
    );
  }
  const { summary, evidenceAssessment, findings, openQuestions } =
    conversationCapabilityPayloadView(artifact.payload);
  const agentTrace = artifact.agent_trace ?? [];
  const hasAnalysisShape = Boolean(summary || findings.length || openQuestions.length);

  return (
    <div className="capability-artifact-body">
      {evidenceAssessment ? <EvidenceAssessment value={evidenceAssessment} /> : null}
      {summary ? <p className="capability-summary">{summary}</p> : null}
      {findings.length ? (
        <ol className="capability-findings">
          {findings.map((finding, index) => (
            <li key={`${artifact.call_id}:finding:${index}`}>
              <header>
                <strong>{finding.title}</strong>
                <span className="capability-finding-badges">
                  {finding.claimType ? (
                    <span className={`claim-type ${finding.claimType}`}>
                      {t(claimTypeKey(finding.claimType))}
                    </span>
                  ) : null}
                  <span className={`confidence ${finding.confidence}`}>
                    {t(confidenceKey(finding.confidence))}
                  </span>
                </span>
              </header>
              <p>{finding.judgment}</p>
              {finding.evidence.length ? (
                <EvidenceClaims values={finding.evidence} />
              ) : null}
              {finding.causalChain.length ? (
                <EvidenceList title={t("home.capabilityCausalChain")} values={finding.causalChain} />
              ) : null}
              {finding.counterargument ? (
                <div className="capability-counterargument">
                  <strong>{t("home.capabilityCounterargument")}</strong>
                  <p>{finding.counterargument}</p>
                </div>
              ) : null}
              {finding.unknowns.length ? (
                <EvidenceList title={t("home.capabilityUnknowns")} values={finding.unknowns} />
              ) : null}
              {finding.leadingIndicators.length ? (
                <EvidenceList
                  title={t("home.capabilityLeadingIndicators")}
                  values={finding.leadingIndicators}
                />
              ) : null}
              {finding.falsification ? (
                <LabeledText title={t("home.capabilityFalsification")} value={finding.falsification} />
              ) : null}
              {finding.decisionImpact ? (
                <LabeledText title={t("home.capabilityDecisionImpact")} value={finding.decisionImpact} />
              ) : null}
            </li>
          ))}
        </ol>
      ) : null}
      {openQuestions.length ? (
        <EvidenceList title={t("home.capabilityOpenQuestions")} values={openQuestions} />
      ) : null}
      {agentTrace.length ? <AgentTrace values={agentTrace} /> : null}
      {!hasAnalysisShape ? (
        <pre className="capability-generic-output">{JSON.stringify(artifact.payload, null, 2)}</pre>
      ) : null}
      {artifact.warning ? <p className="capability-warning">{artifact.warning}</p> : null}
    </div>
  );
}

function EvidenceAssessment({ value }: { value: ConversationCapabilityEvidenceAssessment }) {
  const { t } = useI18n();
  return (
    <div className={`capability-evidence-assessment ${value.status}`}>
      <header>
        <strong>{t("home.capabilityEvidenceAssessment")}</strong>
        <span>{t(evidenceStatusKey(value.status))}</span>
      </header>
      <p>{value.rationale}</p>
      {value.latestEvidenceDate ? (
        <small>{t("home.capabilityLatestEvidence", { date: value.latestEvidenceDate })}</small>
      ) : null}
      {value.criticalGaps.length ? (
        <EvidenceList title={t("home.capabilityCriticalGaps")} values={value.criticalGaps} />
      ) : null}
    </div>
  );
}

function EvidenceClaims({ values }: { values: ConversationCapabilityEvidence[] }) {
  const { t } = useI18n();
  return (
    <div className="capability-evidence capability-evidence-claims">
      <strong>{t("home.capabilityEvidence")}</strong>
      <ul>
        {values.map((value, index) => (
          <li key={`${value.claim}:${index}`}>
            <span>{value.claim}</span>
            {value.sourceUrls.length ? (
              <small>
                {value.sourceUrls.map((url, urlIndex) => {
                  const safeUrl = safeHttpUrl(url);
                  return safeUrl ? (
                    <a href={safeUrl} target="_blank" rel="noreferrer" key={url}>
                      {t("home.capabilitySource", { index: urlIndex + 1 })}
                    </a>
                  ) : null;
                })}
              </small>
            ) : null}
            {value.asOf ? <small>{t("home.capabilityEvidenceAsOf", { date: value.asOf })}</small> : null}
          </li>
        ))}
      </ul>
    </div>
  );
}

function LabeledText({ title, value }: { title: string; value: string }) {
  return (
    <div className="capability-counterargument">
      <strong>{title}</strong>
      <p>{value}</p>
    </div>
  );
}

function AgentTrace({ values }: { values: AgentExecutionTrace[] }) {
  const { t } = useI18n();
  return (
    <div className="capability-agent-trace">
      <strong>{t("home.capabilityAgentTrace")}</strong>
      <ol>
        {values.map((entry) => (
          <li key={`${entry.turn}:${entry.action}:${entry.tool_id ?? "final"}`}>
            <span>{t("home.capabilityAgentTurn", { turn: entry.turn })}</span>
            <p>
              {entry.action === "tool"
                ? t("home.capabilityAgentToolStep", {
                    tool: entry.tool_display_name ?? entry.tool_id ?? "",
                    status: t(
                      entry.status === "completed"
                        ? "home.capabilityAgentStepCompleted"
                        : "home.capabilityAgentStepFailed"
                    ),
                    sources: entry.source_count
                  })
                : t("home.capabilityAgentFinalStep")}
            </p>
          </li>
        ))}
      </ol>
    </div>
  );
}

function capabilityFailureKey(code?: string | null): TranslationKey {
  if (code === "timeout") return "home.capabilityFailedTimeout";
  if (code === "provider_error") return "home.capabilityFailedProvider";
  if (code === "invalid_output") return "home.capabilityFailedInvalidOutput";
  if (code === "unavailable") return "home.capabilityFailedUnavailable";
  return "home.capabilityFailed";
}

function EvidenceList({ title, values }: { title: string; values: string[] }) {
  return (
    <div className="capability-evidence">
      <strong>{title}</strong>
      <ul>
        {values.map((value, index) => <li key={`${title}:${index}`}>{value}</li>)}
      </ul>
    </div>
  );
}

function capabilityName(
  artifact: ConversationCapabilityArtifact,
  t: (key: TranslationKey, values?: Record<string, string | number>) => string
) {
  const names: Record<string, TranslationKey> = {
    analyze_business_model: "home.capabilityBusinessModel",
    audit_moat: "home.capabilityMoatAudit",
    analyze_company: "home.capabilityCompanyAnalysis",
    challenge_company_thesis: "home.capabilityThesisChallenge"
  };
  const key = names[artifact.capability_id];
  return key ? t(key) : artifact.display_name;
}

function capabilityMeta(
  artifact: ConversationCapabilityArtifact,
  t: (key: TranslationKey, values?: Record<string, string | number>) => string
) {
  const kind = t(
    artifact.capability_kind === "agent"
      ? "home.capabilityKindAgent"
      : "home.capabilityKindSkill"
  );
  const execution = t("home.capabilityExecutionMeta", {
    version: artifact.capability_version,
    seconds: Math.max(0.01, artifact.duration_ms / 1_000).toFixed(2),
    steps: artifact.execution_steps
  });
  return [kind, execution, artifact.provider, artifact.model].filter(Boolean).join(" · ");
}

function confidenceKey(confidence: string): TranslationKey {
  if (confidence === "high") return "home.capabilityConfidenceHigh";
  if (confidence === "medium") return "home.capabilityConfidenceMedium";
  return "home.capabilityConfidenceLow";
}

function claimTypeKey(claimType: string): TranslationKey {
  if (claimType === "fact") return "home.capabilityClaimFact";
  if (claimType === "inference") return "home.capabilityClaimInference";
  return "home.capabilityClaimHypothesis";
}

function evidenceStatusKey(status: string): TranslationKey {
  if (status === "sufficient") return "home.capabilityEvidenceSufficient";
  if (status === "partial") return "home.capabilityEvidencePartial";
  return "home.capabilityEvidenceInsufficient";
}
