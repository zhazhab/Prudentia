import type { ReactNode } from "react";

interface EmptyStateProps {
  title: string;
  children: ReactNode;
  action?: ReactNode;
}

export function EmptyState({ title, children, action }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <div className="empty-state-head">
        <strong>{title}</strong>
        {action}
      </div>
      <p>{children}</p>
    </div>
  );
}
