import { FormEvent, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bot, Plus } from "lucide-react";
import { api } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { useI18n } from "../i18n";
import type { Memo, MemoExtraction } from "../types/domain";

const initialForm = {
  title: "",
  symbol: "",
  notes: "",
  tags: ""
};

export function MemosPage() {
  const { languageTag, t } = useI18n();
  const queryClient = useQueryClient();
  const memos = useQuery({ queryKey: ["memos"], queryFn: api.memos });
  const [form, setForm] = useState(initialForm);
  const [selectedMemo, setSelectedMemo] = useState<Memo | null>(null);
  const [extraction, setExtraction] = useState<MemoExtraction | null>(null);

  const createMemo = useMutation({
    mutationFn: api.createMemo,
    onSuccess: (memo) => {
      queryClient.invalidateQueries({ queryKey: ["memos"] });
      queryClient.invalidateQueries({ queryKey: ["profile"] });
      setSelectedMemo(memo);
      setForm(initialForm);
    }
  });

  const extractMemo = useMutation({
    mutationFn: (id: string) => api.extractMemo(id, languageTag),
    onSuccess: setExtraction
  });

  function submit(event: FormEvent) {
    event.preventDefault();
    createMemo.mutate({
      title: form.title,
      symbol: form.symbol || undefined,
      notes: form.notes,
      tags: form.tags
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean)
    });
  }

  return (
    <div className="page-stack">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("memos.eyebrow")}</span>
          <h2>{t("memos.title")}</h2>
        </div>
      </header>

      <section className="editor-grid">
        <form className="panel form-panel" onSubmit={submit}>
          <div className="panel-head">
            <h3>{t("memos.newMemo")}</h3>
          </div>
          <label>
            <span>{t("memos.fieldTitle")}</span>
            <input
              value={form.title}
              onChange={(event) => setForm({ ...form, title: event.target.value })}
              placeholder={t("memos.titlePlaceholder")}
              required
            />
          </label>
          <label>
            <span>{t("memos.fieldSymbol")}</span>
            <input
              value={form.symbol}
              onChange={(event) => setForm({ ...form, symbol: event.target.value })}
              placeholder="AAPL"
            />
          </label>
          <label>
            <span>{t("memos.fieldNotes")}</span>
            <textarea
              value={form.notes}
              onChange={(event) => setForm({ ...form, notes: event.target.value })}
              rows={9}
              placeholder={t("memos.notesPlaceholder")}
            />
          </label>
          <label>
            <span>{t("memos.fieldTags")}</span>
            <input
              value={form.tags}
              onChange={(event) => setForm({ ...form, tags: event.target.value })}
              placeholder={t("memos.tagsPlaceholder")}
            />
          </label>
          <button className="primary-button" type="submit">
            <Plus size={18} />
            {t("memos.create")}
          </button>
        </form>

        <section className="panel">
          <div className="panel-head">
            <h3>{t("memos.library")}</h3>
          </div>
          {(memos.data ?? []).length ? (
            <div className="memo-list">
              {(memos.data ?? []).map((memo) => (
                <button
                  className={selectedMemo?.id === memo.id ? "memo-row active" : "memo-row"}
                  key={memo.id}
                  type="button"
                  onClick={() => {
                    setSelectedMemo(memo);
                    setExtraction(null);
                  }}
                >
                  <div>
                    <strong>{memo.title}</strong>
                    <p>{memo.symbol ?? memo.asset_type}</p>
                  </div>
                  <span className="pill">{memo.status}</span>
                </button>
              ))}
            </div>
          ) : (
            <EmptyState title={t("memos.noMemosTitle")}>{t("memos.noMemosBody")}</EmptyState>
          )}
        </section>
      </section>

      {selectedMemo ? (
        <section className="panel">
          <div className="panel-head">
            <div>
              <h3>{selectedMemo.title}</h3>
              <p>{selectedMemo.symbol ?? t("memos.noSymbol")}</p>
            </div>
            <button
              className="primary-button"
              type="button"
              onClick={() => extractMemo.mutate(selectedMemo.id)}
            >
              <Bot size={18} />
              {t("memos.aiExtract")}
            </button>
          </div>
          <div className="memo-detail-grid">
            <MemoBlock label={t("memos.notes")} value={selectedMemo.notes || t("memos.noNotes")} />
            <MemoBlock label={t("memos.thesis")} value={selectedMemo.thesis || extraction?.thesis || "-"} />
            <MemoBlock label={t("memos.risks")} value={selectedMemo.risks || extraction?.risks || "-"} />
            <MemoBlock label={t("memos.catalysts")} value={selectedMemo.catalysts || extraction?.catalysts || "-"} />
            <MemoBlock
              label={t("memos.disconfirmingEvidence")}
              value={selectedMemo.disconfirming_evidence || extraction?.disconfirming_evidence || "-"}
            />
          </div>
          {extraction ? (
            <div className="checklist-box">
              <strong>{t("memos.aiChecklist")}</strong>
              {extraction.checklist.map((item) => (
                <span key={item}>{item}</span>
              ))}
            </div>
          ) : null}
        </section>
      ) : null}
    </div>
  );
}

function MemoBlock({ label, value }: { label: string; value: string }) {
  return (
    <div className="memo-block">
      <strong>{label}</strong>
      <p>{value}</p>
    </div>
  );
}
