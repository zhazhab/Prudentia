import { Bot, RotateCcw, UserRound } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { MemoThreadMessage } from "../types/domain";
import { useI18n } from "../i18n";
import { usedContextDescriptor } from "../pages/homeRules";

export function ConversationMessage({
  message,
  onRetry
}: {
  message: MemoThreadMessage;
  onRetry?: () => void;
}) {
  const { t } = useI18n();
  const assistant = message.role === "assistant";
  const status =
    message.status === "failed"
      ? t("home.failed")
      : message.status === "canceled"
        ? t("home.canceled")
        : message.status === "streaming"
          ? t("home.streaming")
          : null;

  if (assistant && !message.content) {
    return null;
  }

  return (
    <article className={assistant ? "chat-message assistant" : "chat-message user"}>
      <div className="message-avatar">{assistant ? <Bot size={16} /> : <UserRound size={16} />}</div>
      <div className="message-bubble">
        <div className="message-meta">
          <strong>{assistant ? t("home.assistantLabel") : t("home.userLabel")}</strong>
          {status ? <span>{status}</span> : null}
          {onRetry && ["failed", "canceled"].includes(message.status) ? (
            <button
              type="button"
              className="message-inline-action"
              onClick={onRetry}
              title={t("home.retry")}
              aria-label={t("home.retry")}
            >
              <RotateCcw size={14} />
            </button>
          ) : null}
        </div>
        <div className="message-markdown">
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            skipHtml
            components={{
              a: ({ href, children }) => (
                <a href={href} target="_blank" rel="noreferrer">
                  {children}
                </a>
              )
            }}
          >
            {message.content}
          </ReactMarkdown>
        </div>
        {message.sources.length ? (
          <details className="message-disclosure">
            <summary>{t("home.sources")}</summary>
            <ul>
              {message.sources.map((source, index) => (
                <li key={`${message.id}:source:${index}`}>{sourceLink(source)}</li>
              ))}
            </ul>
          </details>
        ) : null}
        {message.used_context.length ? (
          <details className="message-disclosure">
            <summary>{t("home.usedContext")}</summary>
            <ul>
              {message.used_context.map((item, index) => {
                const descriptor = usedContextDescriptor(item);
                return (
                  <li key={`${message.id}:context:${index}`}>
                    {t(descriptor.key, descriptor.params)}
                  </li>
                );
              })}
            </ul>
          </details>
        ) : null}
      </div>
    </article>
  );
}

function sourceLink(source: unknown) {
  if (!source || typeof source !== "object") {
    return String(source);
  }
  const value = source as Record<string, unknown>;
  const title = String(value.title ?? value.url ?? "Source");
  const url = typeof value.url === "string" ? value.url : null;
  return url ? (
    <a href={url} target="_blank" rel="noreferrer">
      {title}
    </a>
  ) : (
    title
  );
}
