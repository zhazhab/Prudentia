import assert from "node:assert/strict";
import test from "node:test";
import {
  conversationCapabilityPayloadView,
  safeHttpUrl
} from "../src/components/conversationCapabilityRules.ts";

test("capability payload preserves evidence quality and decision fields", () => {
  const view = conversationCapabilityPayloadView({
    evidence_assessment: {
      status: "partial",
      rationale: "The filing covers economics but not competitor retention.",
      latest_evidence_date: "2025-12-31",
      critical_gaps: ["Competitor cohort retention"]
    },
    summary: "The profit pool is attractive but its durability is not yet verified.",
    findings: [{
      category: "competitive_intensity",
      title: "Low switching friction limits pricing power",
      judgment: "Customers can multi-home without material migration cost.",
      claim_type: "fact",
      evidence: [{
        claim: "The company identifies intense competition in its annual filing.",
        source_urls: ["https://example.com/filing"],
        as_of: "2025-12-31"
      }],
      causal_chain: ["Low switching cost", "Multi-homing", "Limited pricing power"],
      counterargument: "Better recommendations can still improve retention.",
      unknowns: ["Cohort retention by channel"],
      confidence: "medium",
      leading_indicators: ["Repeat purchase rate"],
      falsification: "Sustained price increases without retention loss.",
      decision_impact: "Treat pricing power as unverified in the company view."
    }],
    open_questions: ["What is retention after incentives are removed?"]
  });

  assert.equal(view.evidenceAssessment?.status, "partial");
  assert.deepEqual(view.evidenceAssessment?.criticalGaps, ["Competitor cohort retention"]);
  assert.equal(view.findings[0].claimType, "fact");
  assert.deepEqual(view.findings[0].evidence[0].sourceUrls, ["https://example.com/filing"]);
  assert.deepEqual(view.findings[0].causalChain, [
    "Low switching cost",
    "Multi-homing",
    "Limited pricing power"
  ]);
  assert.equal(
    view.findings[0].decisionImpact,
    "Treat pricing power as unverified in the company view."
  );
});

test("capability payload remains readable for legacy string evidence", () => {
  const view = conversationCapabilityPayloadView({
    summary: "Legacy result",
    findings: [{
      title: "Legacy finding",
      judgment: "Still visible after the schema upgrade.",
      evidence: ["Previously stored evidence"],
      counterargument: "",
      unknowns: [],
      confidence: "low"
    }],
    open_questions: []
  });

  assert.equal(view.evidenceAssessment, null);
  assert.deepEqual(view.findings[0].evidence, [{
    claim: "Previously stored evidence",
    sourceUrls: [],
    asOf: ""
  }]);
  assert.deepEqual(view.findings[0].causalChain, []);
});

test("capability source links only allow HTTP protocols", () => {
  assert.equal(safeHttpUrl("https://example.com/filing"), "https://example.com/filing");
  assert.equal(safeHttpUrl("http://example.com/source"), "http://example.com/source");
  assert.equal(safeHttpUrl("javascript:alert(1)"), null);
  assert.equal(safeHttpUrl("file:///tmp/source"), null);
  assert.equal(safeHttpUrl("not a URL"), null);
});
