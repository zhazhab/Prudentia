# Conversation Capabilities

Prudentia calls each reusable unit of conversation work a **Capability**. The Capability Registry manages three kinds:

- `native`: deterministic Rust application tools such as company research and community-insight retrieval. They may access explicitly authorized infrastructure, but a model cannot invent or invoke arbitrary system tools.
- `skill`: a versioned analytical method such as business-model analysis or moat auditing. It can run directly once or be preloaded into an agent as method instructions; it has no independent task lifecycle.
- `agent`: a bounded analysis session with its own instructions, model tier, context permissions, exact-version skills, read-only tool allowlist, and turn limit, such as company deep analysis or dissent analysis.

Skills and agents produce read-only structured artifacts. Company-view, trade, and investment-rule writes still require separate confirmation cards and deterministic domain code.

## Turn Flow

Each turn runs in this order:

1. Resolve the effective subject and create a deterministic `ToolPlan`.
2. Run required native research tools first.
3. Freeze the authorized subject, request, company view, research sources, attachments, and other context.
4. Run matching skills and agents in parallel against that same snapshot. A skill produces a structured artifact directly. An agent loads registered methods and decides each turn between one allowlisted read-only tool call and its final structured result. A skill already preloaded by a lead-analysis agent does not run separately unless the user explicitly requests `@skill_id`. A dissent agent does not read another capability's output, which reduces positive-case anchoring.
5. Give Schema-validated artifacts to the response model and persist them with the assistant message.
6. Extract separately confirmable data actions after the visible response.

Every call emits persisted `tool.started`, `tool.progress`, `tool.completed`, or `tool.failed` events. Active runs also return a snapshot of unfinished calls, so the frontend can recover when refreshed, when an event arrives before the REST response, or after reconnect. One capability failure does not fail the turn and is rendered as a failed artifact. Public state stores only stable categories such as `timeout`, `provider_error`, `invalid_output`, and `unavailable`; raw provider/CLI errors stay in server logs. Only native research failures become an external-verification degradation warning.

An agent's internal tool call still passes through the Registry. The model returns only a tool id, exact version, and arguments; deterministic code revalidates the allowlist, input Schema, read-only and automatic-confirmation policies, timeout, and output Schema. Each turn can call one tool, equivalent calls cannot repeat, one agent can make at most four tool calls, and it must reserve a turn for its final result. A failed tool becomes a bounded observation so the agent can finish with an explicit degradation instead of gaining more permissions.

The provider schema for an agent decision is not open-ended. The runtime builds a strict union from the allowlisted tool input schemas and final artifact schema, so Codex CLI structured output and application JSON Schema validation share the same allowed shapes and undeclared fields cannot cross the Registry boundary.

## Prompt And Evidence Contract

A model capability prompt has three layers. The runtime protocol owns safety, evidence boundaries, and strict JSON. Manifest instructions define only the skill or agent role, method, research policy, and completion criteria. The output schema defines the required deliverable. The shared protocol refers only to fields actually declared by that schema, so it does not assume every custom capability uses the built-in analysis shape. `research_company` and `research_community_insights` are deterministic Rust tools with no model prompt. They accept one conclusion-changing evidence gap, then let `ResearchPlan` produce controlled retrieval.

The four built-in model capabilities have distinct analytical contracts:

- `analyze_business_model` covers offering and customer, value and cash flow, profit pools and costs, unit economics and capital intensity, owner economics, competitive intensity, attacker economics, and five-to-ten-year operating scenarios.
- `audit_moat` covers candidate structural mechanisms, actual competitor constraints, maintenance cost, the cheapest credible breach path, and durability or failure conditions, after filtering false substitutes such as good products, high share, or strong management.
- `analyze_company` combines business model, owner economics, competitive position, moat, management and capital allocation, financial resilience, earning power, and the strongest failure mechanism.
- `challenge_company_thesis` steelmans the positive case before selecting only three to five material failure mechanisms. It does not refer to an unavailable prior draft or manufacture generic risks merely to fill categories.

A built-in artifact starts with `evidence_assessment`, whose status is `sufficient`, `partial`, or `insufficient`. Insufficient evidence does not trigger category filling: the result contains a concise abstention, decisive gaps, open questions, and at most two low-confidence hypotheses that direct the next retrieval step. Partial evidence produces only supported or decision-changing categories; full coverage is reserved for sufficient evidence. Every finding includes `claim_type`, structured `evidence[]`, `causal_chain`, `counterargument`, `unknowns`, `confidence`, `leading_indicators`, `falsification`, and `decision_impact`. A finding labeled `fact` needs at least one evidence URL, and each URL must exactly match one in the frozen turn context or an agent tool observation. Deterministic validation otherwise records the artifact as `invalid_output`. Unverified judgments must be `inference` or `hypothesis`; model memory cannot fill in market share, margins, probabilities, attack costs, or long-range earnings. Community sources can surface disputes, attack paths, and signals to investigate, but cannot prove a fact or moat.

An agent inspects frozen evidence first and calls one allowlisted tool only when one missing fact could change a material conclusion. It cannot retry the same failed path with synonymous wording. If retrieval remains unavailable, it finishes with `partial` or `insufficient`. The frontend renders evidence quality, claim type, sources, causal chains, and falsification conditions directly. Legacy messages whose `evidence[]` contains strings remain visible as uncited historical evidence after the schema upgrade.

## Local Manifests

Custom manifests load from `capabilities/**/*.json` under the shared local root. This is the same root used by `.env`, SQLite, and `data/workspace`. It defaults to the original repository behind the Git common dir, so all worktrees use one capability configuration; `PRUDENTIA_LOCAL_DIR` can override the whole root. `capabilities/` is Git-ignored so personal methods and prompts are not committed accidentally.

Manifests load when the backend starts. Restart the backend after adding or changing one. `GET /api/conversation/capabilities` lists the names, versions, kinds, model tiers, subjects, and surfaces actually registered. It never returns capability instructions.

A minimal company-dissent agent:

```json
{
  "id": "audit_customer_concentration",
  "version": 1,
  "kind": "agent",
  "stage": "challenge",
  "display_name": "Customer concentration dissent audit",
  "description": "Audit concentration, bargaining power, and churn impact",
  "artifact_type": "customer_concentration_audit",
  "instructions": "Use the supplied evidence to identify concentration mechanisms, counterarguments, unknowns, leading indicators, and falsification conditions.",
  "input_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["focus"],
    "properties": {
      "focus": { "type": "string", "maxLength": 4000 }
    }
  },
  "output_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["summary", "open_questions"],
    "properties": {
      "summary": { "type": "string", "maxLength": 6000 },
      "open_questions": {
        "type": "array",
        "maxItems": 12,
        "items": { "type": "string", "maxLength": 800 }
      }
    }
  },
  "context": ["subject", "user_message", "company_view", "research_sources", "attachments"],
  "model": "deep",
  "timeout_seconds": 300,
  "max_steps": 6,
  "tools": [
    { "id": "research_company", "version": 1 },
    { "id": "research_community_insights", "version": 1 }
  ],
  "skills": [
    { "id": "analyze_business_model", "version": 1 },
    { "id": "audit_moat", "version": 1 }
  ],
  "surfaces": ["conversation"],
  "subjects": ["company"],
  "triggers": ["customer concentration", "large customer risk"],
  "initial_activity": "agent_auditing_customer_concentration"
}
```

## Manifest Contract

- `id`, `artifact_type`, and `initial_activity` are snake_case identifiers no longer than 80 bytes.
- `kind` is only `skill` or `agent`; manifests cannot inject native tools.
- `stage` is `analysis` or `challenge` and defaults to `analysis`. Execution form and role are independent: an agent may lead analysis, while a skill may implement a deterministic dissent method. `research` remains reserved for native tools.
- `model` is `simple`, `standard`, or `deep`. The current AI fallback chain and model-tier settings select the actual provider/model.
- A skill must use `max_steps: 1`; an agent may use up to `8`. Every step's actual provider/model is recorded in the artifact's `model_steps`.
- Only agents may declare `tools` and `skills`. References pin an exact `id + version`; an agent may load at most eight tools and four skills. Every dependency must cover all surfaces and subjects declared by the agent. A missing dependency, scope or kind mismatch, or tool not explicitly exposed as automatic and read-only causes the agent to be ignored during startup registration.
- `surfaces` is `conversation`, `rule_graph`, or both. `subjects` is a non-empty subset of `company`, `investment_system`, `psychology`, and `general`.
- A conversation-surface input must require only one string field named `focus`; the planner supplies the original turn request.
- `context` is an explicit least-privilege list: `subject`, `user_message`, `company_view`, `research_sources`, `attachments`, `conversation_history`, `portfolio`, `investment_system`, and `rule_graph_input`. A manifest with the `rule_graph` surface must explicitly grant `rule_graph_input`; otherwise graph execution cannot expose its input to a model capability.
- JSON Schema supports only the subset enforced by the runtime: `type`, `enum`, `properties`, `required`, `additionalProperties`, `items`, `maxItems`, `maxLength`, and `description`. Unknown keywords reject the manifest instead of creating an unenforced security claim.
- A manifest is at most 256 KiB, instructions are at most 32,000 characters, all skill instructions preloaded by one agent total at most 48,000 characters, and timeout is at most 600 seconds. The loader reads at most 64 files to depth 4 and rejects symlinks.
- One structured output is at most 48 KiB. Agent observations sent back to the model are capped at 96 KiB, include at most twelve sources per call, and truncate each source snippet to 1,600 characters. At most three model capabilities run in one turn. Tool events and `agent_trace` store only action, version, status, turn, and source-count metadata; they never duplicate research bodies. The same URL is stored once per run and reuses its source id.
- An invalid file is logged and ignored without suppressing valid neighbors. A directory-structure or symlink violation rejects the custom directory as a whole.

For each `id`, the conversation planner selects the highest version, then matches subject scope and `triggers`. A user can invoke one explicitly with `@capability_id`. An identical `id + version` cannot replace a registered capability. Publish a new version instead of changing old semantics.

## Rule-Graph Reuse

A skill or agent with the `rule_graph` surface registers an exact-version adapter keyed as `id@version`. Example node:

```json
{
  "id": "customer-risk",
  "label": "Customer concentration audit",
  "kind": "agent",
  "operation": "audit_customer_concentration",
  "config": {
    "adapter": "audit_customer_concentration@1",
    "arguments": { "focus": "Audit customer concentration in the current input" },
    "locale": "en-US"
  },
  "input_schema": { "type": "object", "required": ["context", "incoming"] },
  "output_schema": { "type": "object", "required": ["summary", "open_questions"] },
  "x": 320,
  "y": 180
}
```

Before activation, the graph validates adapter availability, node/adapter kind agreement, capability `arguments`, node input/output shapes, and acyclicity. A graph is limited to 64 nodes, 256 edges, 8 model-backed nodes, and 512 KiB serialized. Execution also bounds graph input and node output, has a 600-second total deadline, and retains only the latest 500 executions. The execution input is stored once; each node trace keeps a `context_ref` plus its incoming values and output instead of duplicating the full context.

Activation pins the manifest content hash into node configuration. Evaluation requires both `id@version` and `manifest_hash` to match. If a local manifest is incorrectly edited in place without a version bump, an already activated graph refuses to run it until the capability version is incremented and a new graph version is activated. Graphs never drift to the latest version automatically.

## Extension Boundary

Prefer a skill for a reusable analytical method and an agent when the role must fill evidence gaps, combine multiple methods, or form an independent judgment. A new website, database, file format, or other infrastructure integration requires a reviewed Rust `native` adapter that explicitly defines its model-facing input conversion, cache, timeout, storage, and side-effect policies. A pure community request uses the dedicated community tool; a request that also asks for business model, financials, or moat still runs broad company research so community opinions cannot replace primary and independent evidence. Prudentia does not expose `load_skill(path)`, `spawn_agent(prompt)`, arbitrary commands, Shell, browser, or plugin passthrough. A CLI provider only returns structured agent decisions; Prudentia's Registry and confirmation flow own all tool execution and domain writes.
