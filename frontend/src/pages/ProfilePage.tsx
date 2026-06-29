import { useQuery } from "@tanstack/react-query";
import { ShieldCheck, Sparkles, TriangleAlert } from "lucide-react";
import { api } from "../api/client";
import { EmptyState } from "../components/EmptyState";
import { StatCard } from "../components/StatCard";
import { useI18n } from "../i18n";

export function ProfilePage() {
  const { languageTag, t } = useI18n();
  const profile = useQuery({
    queryKey: ["profile", languageTag],
    queryFn: () => api.profile(languageTag)
  });
  const data = profile.data;

  return (
    <div className="page-stack">
      <header className="page-header">
        <div>
          <span className="eyebrow">{t("profile.eyebrow")}</span>
          <h2>{t("profile.title")}</h2>
        </div>
      </header>

      <section className="stats-grid">
        <StatCard label={t("profile.level")} value={`${data?.level ?? 1}`} icon={<ShieldCheck size={18} />} />
        <StatCard label={t("profile.xp")} value={`${data?.xp ?? 0}`} detail={t("profile.nextLevel", { xp: data?.next_level_xp ?? 100 })} />
        <StatCard label={t("profile.badges")} value={`${data?.badges.length ?? 0}`} icon={<Sparkles size={18} />} />
        <StatCard label={t("profile.biasSignals")} value={`${data?.bias_signals.length ?? 0}`} icon={<TriangleAlert size={18} />} />
      </section>

      <section className="panel">
        <div className="panel-head">
          <h3>{t("profile.attributes")}</h3>
        </div>
        <div className="attribute-list">
          {(data?.attributes ?? []).map((attribute) => (
            <div className="attribute-card" key={attribute.name}>
              <div className="attribute-card-head">
                <strong>{attribute.name}</strong>
                <span>{attribute.score}</span>
              </div>
              <div className="meter">
                <span style={{ width: `${attribute.score}%` }} />
              </div>
              <p>{attribute.description}</p>
            </div>
          ))}
        </div>
      </section>

      <section className="dashboard-grid">
        <div className="panel">
          <div className="panel-head">
            <h3>{t("profile.badges")}</h3>
          </div>
          {(data?.badges ?? []).length ? (
            <div className="badge-list">
              {(data?.badges ?? []).map((badge) => (
                <div className="badge-card" key={badge.name}>
                  <strong>{badge.name}</strong>
                  <p>{badge.description}</p>
                </div>
              ))}
            </div>
          ) : (
            <EmptyState title={t("profile.noBadgesTitle")}>{t("profile.noBadgesBody")}</EmptyState>
          )}
        </div>

        <div className="panel">
          <div className="panel-head">
            <h3>{t("profile.biasSignals")}</h3>
          </div>
          {(data?.bias_signals ?? []).length ? (
            <div className="warning-list">
              {(data?.bias_signals ?? []).map((signal) => (
                <div className="warning-box" key={signal}>{signal}</div>
              ))}
            </div>
          ) : (
            <EmptyState title={t("profile.noSignalsTitle")}>{t("profile.noSignalsBody")}</EmptyState>
          )}
        </div>
      </section>

      <section className="panel">
        <div className="panel-head">
          <h3>{t("profile.ruleEvents")}</h3>
        </div>
        <div className="event-list">
          {(data?.rule_events ?? []).map((event) => (
            <span key={event}>{event}</span>
          ))}
        </div>
      </section>
    </div>
  );
}
