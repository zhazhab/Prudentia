import type { ReactNode } from "react";

interface StatCardProps {
  label: string;
  value: string;
  detail?: string;
  tone?: "neutral" | "positive" | "warning";
  icon?: ReactNode;
}

export function StatCard({ label, value, detail, tone = "neutral", icon }: StatCardProps) {
  return (
    <section className={`stat-card ${tone}`}>
      <div className="stat-card-head">
        <span>{label}</span>
        {icon}
      </div>
      <strong>{value}</strong>
      {detail ? <p>{detail}</p> : null}
    </section>
  );
}
