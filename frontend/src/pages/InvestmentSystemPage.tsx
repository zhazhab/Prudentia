import { FormEvent, useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bot, Save } from "lucide-react";
import { api } from "../api/client";
import { useI18n } from "../i18n";
import type { InvestmentSystem } from "../types/domain";

export function InvestmentSystemPage() {
  const { languageTag, t } = useI18n();
  const queryClient = useQueryClient();
  const system = useQuery({
    queryKey: ["investment-system", languageTag],
    queryFn: () => api.investmentSystem(languageTag)
  });
  const [draft, setDraft] = useState({
    principles: "",
    checklist_items: "",
    circle_of_competence: "",
    decision_rules: ""
  });

  useEffect(() => {
    if (!system.data) {
      return;
    }
    setDraft({
      principles: system.data.principles.join("\n"),
      checklist_items: system.data.checklist_items.join("\n"),
      circle_of_competence: system.data.circle_of_competence.join("\n"),
      decision_rules: system.data.decision_rules.join("\n")
    });
  }, [system.data]);

  const save = useMutation({
    mutationFn: api.updateInvestmentSystem,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["investment-system"] })
  });

  const refine = useMutation({ mutationFn: () => api.refineInvestmentSystem(languageTag) });

  function submit(event: FormEvent) {
    event.preventDefault();
    const payload: Partial<InvestmentSystem> = {
      principles: lines(draft.principles),
      checklist_items: lines(draft.checklist_items),
      circle_of_competence: lines(draft.circle_of_competence),
      decision_rules: lines(draft.decision_rules)
    };
    save.mutate(payload);
  }

  return (
    <div className="page-stack">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("system.eyebrow")}</span>
          <h2>{t("system.title")}</h2>
        </div>
        <button className="primary-button" type="button" onClick={() => refine.mutate()}>
          <Bot size={18} />
          {t("system.aiRefine")}
        </button>
      </header>

      <form className="panel system-form" onSubmit={submit}>
        <SystemTextarea
          label={t("system.principles")}
          value={draft.principles}
          onChange={(value) => setDraft({ ...draft, principles: value })}
        />
        <SystemTextarea
          label={t("system.checklist")}
          value={draft.checklist_items}
          onChange={(value) => setDraft({ ...draft, checklist_items: value })}
        />
        <SystemTextarea
          label={t("system.circle")}
          value={draft.circle_of_competence}
          onChange={(value) => setDraft({ ...draft, circle_of_competence: value })}
        />
        <SystemTextarea
          label={t("system.decisionRules")}
          value={draft.decision_rules}
          onChange={(value) => setDraft({ ...draft, decision_rules: value })}
        />
        <button className="primary-button" type="submit">
          <Save size={18} />
          {t("system.save")}
        </button>
      </form>

      {refine.data ? (
        <section className="panel">
          <div className="panel-head">
            <h3>{t("system.aiRefinement")}</h3>
          </div>
          <p className="summary-copy">{refine.data.summary}</p>
          <div className="signal-grid">
            {refine.data.checklist_items.map((item) => (
              <div className="attribute-strip" key={item}>
                <strong>{item}</strong>
              </div>
            ))}
          </div>
        </section>
      ) : null}
    </div>
  );
}

function SystemTextarea({
  label,
  value,
  onChange
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label>
      <span>{label}</span>
      <textarea value={value} onChange={(event) => onChange(event.target.value)} rows={7} />
    </label>
  );
}

function lines(value: string) {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}
