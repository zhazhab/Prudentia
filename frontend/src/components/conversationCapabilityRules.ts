import type {
  ConversationCapabilityEvidence,
  ConversationCapabilityEvidenceAssessment,
  ConversationCapabilityFinding
} from "../types/domain";

export interface ConversationCapabilityPayloadView {
  summary: string;
  evidenceAssessment: ConversationCapabilityEvidenceAssessment | null;
  findings: ConversationCapabilityFinding[];
  openQuestions: string[];
}

export function conversationCapabilityPayloadView(
  payload: Record<string, unknown>
): ConversationCapabilityPayloadView {
  return {
    summary: textValue(payload.summary),
    evidenceAssessment: evidenceAssessmentValue(payload.evidence_assessment),
    findings: findingValues(payload.findings),
    openQuestions: textArray(payload.open_questions)
  };
}

export function safeHttpUrl(value: string) {
  try {
    const parsed = new URL(value);
    return parsed.protocol === "https:" || parsed.protocol === "http:" ? parsed.toString() : null;
  } catch {
    return null;
  }
}

function findingValues(value: unknown): ConversationCapabilityFinding[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const finding = item as Record<string, unknown>;
    const title = textValue(finding.title);
    const judgment = textValue(finding.judgment);
    if (!title || !judgment) return [];
    return [{
      category: textValue(finding.category),
      title,
      judgment,
      claimType: textValue(finding.claim_type),
      evidence: evidenceValues(finding.evidence),
      causalChain: textArray(finding.causal_chain),
      counterargument: textValue(finding.counterargument),
      unknowns: textArray(finding.unknowns),
      confidence: textValue(finding.confidence) || "low",
      leadingIndicators: textArray(finding.leading_indicators),
      falsification: textValue(finding.falsification),
      decisionImpact: textValue(finding.decision_impact)
    }];
  });
}

function evidenceValues(value: unknown): ConversationCapabilityEvidence[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (typeof item === "string" && item.trim()) {
      return [{ claim: item, sourceUrls: [], asOf: "" }];
    }
    if (!item || typeof item !== "object") return [];
    const evidence = item as Record<string, unknown>;
    const claim = textValue(evidence.claim);
    if (!claim) return [];
    return [{
      claim,
      sourceUrls: textArray(evidence.source_urls),
      asOf: textValue(evidence.as_of)
    }];
  });
}

function evidenceAssessmentValue(value: unknown): ConversationCapabilityEvidenceAssessment | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const assessment = value as Record<string, unknown>;
  const status = textValue(assessment.status);
  const rationale = textValue(assessment.rationale);
  if (!status || !rationale) return null;
  return {
    status,
    rationale,
    latestEvidenceDate: textValue(assessment.latest_evidence_date),
    criticalGaps: textArray(assessment.critical_gaps)
  };
}

function textValue(value: unknown) {
  return typeof value === "string" ? value : "";
}

function textArray(value: unknown) {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string" && Boolean(item.trim()))
    : [];
}
