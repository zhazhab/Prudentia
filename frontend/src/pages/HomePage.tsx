import {
  Fragment,
  type DragEvent,
  type FormEvent,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Archive,
  Check,
  LoaderCircle,
  MoreHorizontal,
  PanelLeftOpen,
  PanelRightOpen,
  Plus,
  RotateCcw,
  Send,
  Square,
  Trash2,
  X
} from "lucide-react";
import { api } from "../api/client";
import { useConversationEvents } from "../api/useConversationEvents";
import { ConversationActionCard } from "../components/ConversationActionCard";
import { ConversationContextPanel } from "../components/ConversationContextPanel";
import { ConversationMessage } from "../components/ConversationMessage";
import { EmptyState } from "../components/EmptyState";
import { useI18n } from "../i18n";
import type {
  ConversationAction,
  ConversationAttachment,
  ConversationRun,
  ConversationRunPhase,
  ConversationThreadSummary,
  MemoThreadMessage,
  RunEvent
} from "../types/domain";
import {
  chatHomeDefaultThreadId,
  formatThreadTime,
  type LiveConversationRun,
  mergeStoredActiveRun,
  mergeConversationMessages,
  memoChatElapsedSeconds,
  placeConversationActions,
  parseTaskRouteReason,
  runActivityDescriptor,
  taskComplexityKey,
  taskRouteReasonKey,
  shouldScrollConversationToBottom,
  shouldSubmitComposerMessage,
  shortThreadTitle,
  threadRailItems
} from "./homeRules";

type LiveRun = LiveConversationRun;

export function HomePage() {
  const { languageTag, locale, t } = useI18n();
  const queryClient = useQueryClient();
  const { client: eventClient, connectionError } = useConversationEvents();
  const threads = useQuery({ queryKey: ["conversation-threads"], queryFn: api.conversationThreads });
  const activeRuns = useQuery({ queryKey: ["conversation-runs", "active"], queryFn: api.activeConversationRuns });
  const positions = useQuery({ queryKey: ["positions"], queryFn: () => api.positions() });
  const summary = useQuery({ queryKey: ["portfolio-summary"], queryFn: api.portfolioSummary });
  const [activeThreadId, setActiveThreadId] = useState<string | null>(null);
  const [input, setInput] = useState("");
  const [attachments, setAttachments] = useState<ConversationAttachment[]>([]);
  const [uploading, setUploading] = useState(false);
  const [threadMenuId, setThreadMenuId] = useState<string | null>(null);
  const [runsByThread, setRunsByThread] = useState<Record<string, LiveRun>>({});
  const [optimisticMessages, setOptimisticMessages] = useState<Record<string, MemoThreadMessage[]>>({});
  const [runtimeNow, setRuntimeNow] = useState(() => Date.now());
  const [editingSubject, setEditingSubject] = useState(false);
  const [subjectKind, setSubjectKind] = useState("general");
  const [subjectKey, setSubjectKey] = useState("");
  const [subjectLabel, setSubjectLabel] = useState("");
  const [busyActionId, setBusyActionId] = useState<string | null>(null);
  const [mobilePanel, setMobilePanel] = useState<"threads" | "context" | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const messageListRef = useRef<HTMLDivElement | null>(null);
  const pinnedThreadRef = useRef<string | null>(null);

  const activeDetail = useQuery({
    queryKey: ["conversation-thread", activeThreadId],
    queryFn: () => api.conversationThread(activeThreadId ?? ""),
    enabled: Boolean(activeThreadId && !isClientThreadId(activeThreadId))
  });
  const railThreads = useMemo(() => threadRailItems(threads.data ?? []), [threads.data]);
  const activeRun = activeThreadId
    ? runsByThread[activeThreadId] ??
      maybeLiveRun(activeDetail.data?.thread.active_run ?? activeDetail.data?.latest_run)
    : undefined;
  const activeRunning = activeRun && ["queued", "running"].includes(activeRun.status);
  const messages = useMemo(
    () =>
      mergeConversationMessages(
        activeDetail.data?.messages ?? [],
        optimisticMessages[activeThreadId ?? ""] ?? [],
        activeRun
      ),
    [activeDetail.data?.messages, activeRun, activeThreadId, optimisticMessages]
  );
  const actionPlacement = useMemo(
    () => placeConversationActions(messages, activeDetail.data?.actions ?? []),
    [activeDetail.data?.actions, messages]
  );

  useEffect(() => {
    if (!threads.data) return;
    setActiveThreadId((current) => {
      if (current && (isClientThreadId(current) || threads.data.some((thread) => thread.id === current))) {
        return current;
      }
      return chatHomeDefaultThreadId(threads.data);
    });
  }, [threads.data]);

  useEffect(() => {
    if (!activeRuns.data) return;
    setRunsByThread((current) => {
      const next = { ...current };
      activeRuns.data.forEach((run) => {
        next[run.thread_id] = mergeStoredActiveRun(toLiveRun(run), next[run.thread_id]);
      });
      return next;
    });
  }, [activeRuns.data]);

  useEffect(() => eventClient.onEvent(handleEvent), [eventClient]);

  useEffect(() => {
    if (!Object.values(runsByThread).some((run) => ["queued", "running"].includes(run.status))) return;
    const timer = window.setInterval(() => setRuntimeNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [runsByThread]);

  useEffect(() => {
    const list = messageListRef.current;
    if (!list) return;
    const threadId = activeDetail.data?.thread.id ?? activeThreadId;
    const shouldScroll = shouldScrollConversationToBottom({
      threadId,
      pinnedThreadId: pinnedThreadRef.current,
      messageCount: messages.length,
      distanceFromBottom: list.scrollHeight - list.scrollTop - list.clientHeight
    });
    if (shouldScroll) list.scrollTop = list.scrollHeight;
    if (threadId && messages.length) pinnedThreadRef.current = threadId;
  }, [activeDetail.data?.thread.id, activeRun?.streamContent, activeThreadId, messages.length]);

  useEffect(() => {
    if (!mobilePanel) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  }, [mobilePanel]);

  useEffect(() => {
    const subject = activeDetail.data?.thread.subject;
    if (!subject || editingSubject) return;
    setSubjectKind(subject.kind);
    setSubjectKey(subject.subject_key ?? "");
    setSubjectLabel(subject.label ?? "");
  }, [activeDetail.data?.thread.subject, editingSubject]);

  const archiveThread = useMutation({
    mutationFn: api.archiveConversationThread,
    onSuccess: (_thread, id) => {
      void queryClient.invalidateQueries({ queryKey: ["conversation-threads"] });
      if (activeThreadId === id) setActiveThreadId(null);
      setThreadMenuId(null);
    }
  });
  const deleteThread = useMutation({
    mutationFn: api.deleteConversationThread,
    onSuccess: (_thread, id) => {
      void queryClient.invalidateQueries({ queryKey: ["conversation-threads"] });
      queryClient.removeQueries({ queryKey: ["conversation-thread", id] });
      if (activeThreadId === id) setActiveThreadId(null);
      setThreadMenuId(null);
    }
  });

  function handleEvent(event: RunEvent) {
    setRunsByThread((current) => reduceRunEvent(current, event));
    if (["action.proposed", "action.updated", "source.added", "run.warning"].includes(event.event_type)) {
      void queryClient.invalidateQueries({ queryKey: ["conversation-thread", event.thread_id] });
    }
    if (["run.completed", "run.failed", "run.canceled", "run.interrupted"].includes(event.event_type)) {
      void queryClient.invalidateQueries({ queryKey: ["conversation-thread", event.thread_id] });
      void queryClient.invalidateQueries({ queryKey: ["conversation-threads"] });
      void queryClient.invalidateQueries({ queryKey: ["conversation-runs", "active"] });
    }
    if (event.event_type === "action.updated") {
      void queryClient.invalidateQueries({ queryKey: ["positions"] });
      void queryClient.invalidateQueries({ queryKey: ["portfolio-summary"] });
    }
  }

  function startNewThread() {
    const clientId = `client:${makeId()}`;
    setActiveThreadId(clientId);
    setOptimisticMessages((current) => ({ ...current, [clientId]: [] }));
    setInput("");
    setAttachments([]);
    setThreadMenuId(null);
    setMobilePanel(null);
  }

  async function submitChat(event: FormEvent) {
    event.preventDefault();
    if (activeRunning && activeRun) {
      await stopRun(activeRun.id);
      return;
    }
    const content = input.trim();
    if (!content || uploading) return;
    const existingThreadId = activeThreadId && !isClientThreadId(activeThreadId) ? activeThreadId : undefined;
    const localThreadId = activeThreadId ?? `client:${makeId()}`;
    const requestId = `conversation:${makeId()}`;
    appendOptimistic(localThreadId, optimisticUserMessage(localThreadId, requestId, content));
    setInput("");
    try {
      const response = await api.startConversationRun({
        client_request_id: requestId,
        thread_id: existingThreadId,
        client_thread_id: existingThreadId ? undefined : localThreadId,
        content,
        attachment_ids: attachments.map((attachment) => attachment.id),
        locale: languageTag
      });
      moveOptimistic(localThreadId, response.run.thread_id);
      setRunsByThread((current) => ({
        ...current,
        [response.run.thread_id]: toLiveRun(response.run)
      }));
      setActiveThreadId(response.run.thread_id);
      setAttachments([]);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["conversation-threads"] }),
        queryClient.invalidateQueries({ queryKey: ["conversation-thread", response.run.thread_id] })
      ]);
    } catch (error) {
      appendOptimistic(localThreadId, failedAssistantMessage(localThreadId, requestId, error));
    }
  }

  async function stopRun(runId: string) {
    await api.cancelConversationRun(runId);
  }

  async function retryRun(runId: string) {
    const response = await api.retryConversationRun(runId);
    setRunsByThread((current) => ({ ...current, [response.run.thread_id]: toLiveRun(response.run) }));
    await queryClient.invalidateQueries({ queryKey: ["conversation-thread", response.run.thread_id] });
  }

  async function uploadFiles(files: FileList | File[]) {
    setUploading(true);
    try {
      const uploaded = [] as ConversationAttachment[];
      for (const file of Array.from(files)) {
        uploaded.push(
          await api.uploadConversationAttachment({
            file_name: file.name,
            mime_type: file.type || "application/octet-stream",
            content: await fileBase64(file),
            content_encoding: "base64"
          })
        );
      }
      setAttachments((current) => [...current, ...uploaded.filter((item) => !current.some((existing) => existing.id === item.id))]);
    } finally {
      setUploading(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  }

  function handleDrop(event: DragEvent<HTMLFormElement>) {
    event.preventDefault();
    if (event.dataTransfer.files.length) void uploadFiles(event.dataTransfer.files);
  }

  async function saveSubject() {
    if (!activeThreadId || isClientThreadId(activeThreadId)) return;
    await api.updateConversationSubject(activeThreadId, {
      kind: subjectKind,
      subject_key: subjectKind === "company" ? subjectKey : subjectKind === "investment_system" ? "default" : null,
      label: subjectLabel || null
    });
    setEditingSubject(false);
    await queryClient.invalidateQueries({ queryKey: ["conversation-thread", activeThreadId] });
  }

  async function mutateAction(id: string, operation: () => Promise<unknown>) {
    setBusyActionId(id);
    try {
      await operation();
      if (activeThreadId) await queryClient.invalidateQueries({ queryKey: ["conversation-thread", activeThreadId] });
    } finally {
      setBusyActionId(null);
    }
  }

  function appendOptimistic(threadId: string, message: MemoThreadMessage) {
    setOptimisticMessages((current) => ({ ...current, [threadId]: [...(current[threadId] ?? []), message] }));
  }

  function moveOptimistic(from: string, to: string) {
    if (from === to) return;
    setOptimisticMessages((current) => {
      const next = { ...current };
      const moved = (next[from] ?? []).map((message) => ({ ...message, thread_id: to }));
      delete next[from];
      next[to] = [...(next[to] ?? []), ...moved];
      return next;
    });
  }

  function renderActionCard(action: ConversationAction) {
    return (
      <ConversationActionCard
        key={action.id}
        action={action}
        companyView={activeDetail.data?.company_view}
        positions={positions.data ?? []}
        busy={busyActionId === action.id}
        onEdit={(payload) => mutateAction(action.id, () => api.updateConversationAction(action.id, payload))}
        onConfirm={() => mutateAction(action.id, () => api.confirmConversationAction(action.id, action.target_version))}
        onReject={() => mutateAction(action.id, () => api.rejectConversationAction(action.id))}
      />
    );
  }

  return (
    <div className={`home-workspace conversation-home${mobilePanel ? " drawer-open" : ""}`}>
      {mobilePanel ? (
        <button
          className="mobile-drawer-backdrop"
          type="button"
          aria-label={t("home.closePanel")}
          onClick={() => setMobilePanel(null)}
        />
      ) : null}
      <aside
        className={`thread-rail mobile-drawer mobile-drawer-left${mobilePanel === "threads" ? " mobile-open" : ""}`}
        aria-label={t("home.threads")}
      >
        <div className="thread-rail-head">
          <div>
            <span className="eyebrow">{t("home.eyebrow")}</span>
            <h2>{t("home.threads")}</h2>
          </div>
          <div className="thread-rail-controls">
            <button className="icon-button" type="button" onClick={startNewThread} title={t("home.newThread")} aria-label={t("home.newThread")}>
              <Plus size={18} />
            </button>
            <button className="icon-button mobile-drawer-close" type="button" onClick={() => setMobilePanel(null)} title={t("home.closePanel")} aria-label={t("home.closePanel")}>
              <X size={18} />
            </button>
          </div>
        </div>
        <div className="thread-list">
          {railThreads.map((thread) => {
            const run = runsByThread[thread.id] ?? maybeLiveRun(thread.active_run);
            return (
              <div className={activeThreadId === thread.id ? "thread-row active" : "thread-row"} key={thread.id}>
                <button className="thread-row-main" type="button" onClick={() => { setActiveThreadId(thread.id); setMobilePanel(null); }}>
                  <strong>{shortThreadTitle(thread.title)}</strong>
                  <span>
                    {run && ["queued", "running"].includes(run.status) ? <LoaderCircle className="thread-running" size={12} /> : null}
                    {formatThreadTime(thread.last_message_at, locale === "zh" ? "zh-CN" : "en-US")}
                  </span>
                </button>
                <button className="thread-row-menu-button" type="button" title={t("home.moreThreadActions")} aria-label={t("home.moreThreadActions")} onClick={() => setThreadMenuId(threadMenuId === thread.id ? null : thread.id)}>
                  <MoreHorizontal size={16} />
                </button>
                {threadMenuId === thread.id ? (
                  <div className="thread-menu">
                    <button type="button" onClick={() => archiveThread.mutate(thread.id)} title={t("home.archiveThread")}><Archive size={15} /></button>
                    <button type="button" onClick={() => deleteThread.mutate(thread.id)} title={t("home.deleteThread")}><Trash2 size={15} /></button>
                  </div>
                ) : null}
              </div>
            );
          })}
          {!railThreads.length ? <EmptyState title={t("home.noThreadTitle")}>{t("home.noThreadBody")}</EmptyState> : null}
        </div>
      </aside>

      <section className="chat-panel" aria-label={t("home.chatTitle")}>
        <header className="chat-panel-head conversation-head">
          <div>
            <h2>{activeDetail.data?.thread.title ?? t("home.chatTitle")}</h2>
            <p>{activeDetail.data?.thread.subject.label ?? t("home.chatSubtitle")}</p>
          </div>
          {activeDetail.data && !editingSubject ? (
            <button className="subject-control" type="button" onClick={() => setEditingSubject(true)} title={t("home.correctSubject")}>
              {subjectLabelFor(activeDetail.data.thread.subject.kind, t)}
            </button>
          ) : null}
          {editingSubject ? (
            <div className="subject-editor">
              <select value={subjectKind} onChange={(event) => setSubjectKind(event.target.value)} aria-label={t("home.subject")}>
                <option value="general">{t("home.subjectGeneral")}</option>
                <option value="company">{t("home.subjectCompany")}</option>
                <option value="investment_system">{t("home.subjectSystem")}</option>
                <option value="psychology">{t("home.subjectPsychology")}</option>
              </select>
              {subjectKind === "company" ? <input value={subjectKey} onChange={(event) => setSubjectKey(event.target.value.toUpperCase())} placeholder="Symbol" /> : null}
              <input value={subjectLabel} onChange={(event) => setSubjectLabel(event.target.value)} placeholder={t("home.subject")} />
              <button type="button" onClick={() => void saveSubject()} title={t("home.actionSaveEdit")}><Check size={15} /></button>
              <button type="button" onClick={() => setEditingSubject(false)} title={t("common.cancel")}><X size={15} /></button>
            </div>
          ) : null}
          <div className="mobile-chat-controls">
            <button className="mobile-threads-trigger" type="button" onClick={() => setMobilePanel("threads")} title={t("home.openThreads")} aria-label={t("home.openThreads")}>
              <PanelLeftOpen size={18} />
            </button>
            <button className="mobile-context-trigger" type="button" onClick={() => setMobilePanel("context")} title={t("home.openContext")} aria-label={t("home.openContext")}>
              <PanelRightOpen size={18} />
            </button>
          </div>
        </header>

        <div className="message-list" ref={messageListRef}>
          {messages.length ? (
            messages.map((message) => {
              const run = Object.values(runsByThread).find((candidate) => candidate.client_request_id === message.request_id);
              return (
                <Fragment key={message.id}>
                  <ConversationMessage message={message} onRetry={run && ["failed", "canceled", "interrupted"].includes(run.status) ? () => void retryRun(run.id) : undefined} />
                  {(actionPlacement.byMessageId[message.id] ?? []).map(renderActionCard)}
                </Fragment>
              );
            })
          ) : activeDetail.isLoading ? (
            <div className="chat-empty">{t("home.loadingThread")}</div>
          ) : (
            <div className="chat-empty"><strong>{t("home.noActiveThreadTitle")}</strong></div>
          )}

          {activeRun && activeRun.status !== "completed" ? (
            <RunStatus run={activeRun} now={runtimeNow} onRetry={() => void retryRun(activeRun.id)} />
          ) : null}

          {actionPlacement.unplacedActive.map(renderActionCard)}
        </div>

        {connectionError ? <div className="warning-box">{t("home.connectionError", { error: connectionError })}</div> : null}
        <form className="chat-composer conversation-composer" onSubmit={submitChat} onDrop={handleDrop} onDragOver={(event) => event.preventDefault()}>
          {attachments.length || uploading ? (
            <div className="composer-attachments">
              {attachments.map((attachment) => (
                <span key={attachment.id} className={attachment.parse_status === "failed" ? "failed" : ""}>
                  {attachment.file_name}
                  <button type="button" onClick={() => setAttachments((current) => current.filter((item) => item.id !== attachment.id))} aria-label={t("common.cancel")}><X size={12} /></button>
                </span>
              ))}
              {uploading ? <span><LoaderCircle className="backend-runtime-spinner" size={13} />{t("home.attachmentUploading")}</span> : null}
            </div>
          ) : null}
          <input ref={fileInputRef} className="hidden-file-input" type="file" multiple onChange={(event) => event.target.files && void uploadFiles(event.target.files)} />
          <button className="composer-plus" type="button" onClick={() => fileInputRef.current?.click()} title={t("home.attach")} aria-label={t("home.attach")} disabled={uploading}>
            <Plus size={18} />
          </button>
          <label>
            <span>{t("home.inputLabel")}</span>
            <textarea value={input} rows={3} placeholder={t("home.placeholder")} onChange={(event) => setInput(event.target.value)} onKeyDown={(event) => {
              if (shouldSubmitComposerMessage({
                key: event.key,
                shiftKey: event.shiftKey,
                isComposing: event.nativeEvent.isComposing,
                keyCode: event.keyCode
              })) {
                event.preventDefault();
                event.currentTarget.form?.requestSubmit();
              }
            }} />
          </label>
          <button className={activeRunning ? "composer-action stopping" : "composer-action"} type="submit" title={activeRunning ? t("home.stop") : t("home.send")} aria-label={activeRunning ? t("home.stop") : t("home.send")} disabled={!activeRunning && (!input.trim() || uploading)}>
            {activeRunning ? <Square size={18} /> : <Send size={18} />}
          </button>
        </form>
      </section>

      <ConversationContextPanel
        positions={positions.data ?? []}
        summary={summary.data}
        companyView={activeDetail.data?.company_view}
        messages={messages}
        loading={positions.isLoading || summary.isLoading}
        mobileOpen={mobilePanel === "context"}
        onMobileClose={() => setMobilePanel(null)}
      />
    </div>
  );
}

function RunStatus({ run, now, onRetry }: { run: LiveRun; now: number; onRetry: () => void }) {
  const { t } = useI18n();
  const terminal = ["failed", "canceled", "interrupted"].includes(run.status);
  const activity = runActivityDescriptor(run);
  const complexityKey = taskComplexityKey(run.task_complexity);
  const reasonKey = taskRouteReasonKey(run.route_reason);
  const routeParts = [
    complexityKey ? t(complexityKey) : null,
    run.model ?? null,
    run.provider ? t("home.runProvider", { provider: run.provider }) : null,
    t("home.runtimeElapsed", {
      seconds: memoChatElapsedSeconds(
        Date.parse(run.started_at),
        run.finished_at ? Date.parse(run.finished_at) : now
      )
    })
  ].filter((part): part is string => Boolean(part));
  return (
    <div className={`conversation-run-status ${terminal ? "terminal" : ""}`} role="status">
      {terminal ? null : <LoaderCircle className="backend-runtime-spinner" size={17} />}
      <div>
        <strong>
          {run.status === "interrupted"
            ? t("home.runInterrupted")
            : run.status === "canceled"
              ? t("home.runCanceled")
              : run.status === "failed"
                ? t("home.runFailed")
                : t(activity.key, activity.params)}
        </strong>
        <span>{routeParts.join(" · ")}</span>
        {reasonKey && !terminal ? <em>{t(reasonKey)}</em> : null}
        {run.error_message ? <em>{run.error_message}</em> : null}
      </div>
      {terminal ? <button type="button" onClick={onRetry} title={t("home.retry")}><RotateCcw size={15} /></button> : null}
    </div>
  );
}

function reduceRunEvent(current: Record<string, LiveRun>, event: RunEvent) {
  const existing = current[event.thread_id] ?? eventRun(event);
  if (!existing) return current;
  const next: LiveRun = { ...existing };
  if (event.event_type === "run.phase") {
    next.status = "running";
    next.phase = String(event.payload.phase ?? next.phase) as ConversationRunPhase;
    next.provider = typeof event.payload.provider === "string" ? event.payload.provider : next.provider;
    const detail = event.payload.detail && typeof event.payload.detail === "object"
      ? event.payload.detail as Record<string, unknown>
      : undefined;
    next.providerStage = typeof event.payload.provider_stage === "string"
      ? event.payload.provider_stage
      : typeof detail?.activity === "string"
        ? detail.activity
        : next.providerStage;
    next.sourceCount = typeof detail?.source_count === "number"
      ? detail.source_count
      : next.sourceCount;
  } else if (event.event_type === "run.classified" || event.event_type === "run.routed") {
    next.task_complexity = typeof event.payload.task_complexity === "string"
      ? event.payload.task_complexity
      : next.task_complexity;
    next.route_reason = parseTaskRouteReason(event.payload.route_reason) ?? next.route_reason;
    next.model = typeof event.payload.model === "string" ? event.payload.model : next.model;
    next.provider = typeof event.payload.provider === "string"
      ? event.payload.provider
      : next.provider;
  } else if (event.event_type === "message.delta") {
    next.streamContent += String(event.payload.content ?? "");
    next.messageId = typeof event.payload.message_id === "string" ? event.payload.message_id : next.messageId;
  } else if (event.event_type === "message.completed") {
    if (typeof event.payload.content === "string") next.streamContent = event.payload.content;
    next.messageId = typeof event.payload.message_id === "string" ? event.payload.message_id : next.messageId;
  } else if (event.event_type.startsWith("run.")) {
    const status = event.event_type.slice(4);
    if (["completed", "failed", "canceled", "interrupted"].includes(status)) {
      next.status = status as ConversationRun["status"];
      next.phase = status as ConversationRunPhase;
      next.error_message = typeof event.payload.message === "string" ? event.payload.message : next.error_message;
    }
  }
  next.updated_at = event.created_at;
  return { ...current, [event.thread_id]: next };
}

function eventRun(event: RunEvent): LiveRun | null {
  const run = event.payload.run;
  return run && typeof run === "object" ? toLiveRun(run as unknown as ConversationRun) : null;
}

function toLiveRun(run: ConversationRun): LiveRun {
  return {
    ...run,
    streamContent: "",
    providerStage: run.activity ?? undefined,
    sourceCount: run.source_count ?? undefined
  };
}

function maybeLiveRun(run?: ConversationRun | null): LiveRun | undefined {
  return run ? toLiveRun(run) : undefined;
}

function optimisticUserMessage(threadId: string, requestId: string, content: string): MemoThreadMessage {
  const now = new Date().toISOString();
  return { id: `local-user:${requestId}`, thread_id: threadId, role: "user", content, status: "completed", request_id: requestId, duration_ms: null, artifacts: [], sources: [], used_context: [], created_at: now, updated_at: now };
}

function failedAssistantMessage(threadId: string, requestId: string, error: unknown): MemoThreadMessage {
  const now = new Date().toISOString();
  return { id: `local-failed:${requestId}`, thread_id: threadId, role: "assistant", content: error instanceof Error ? error.message : String(error), status: "failed", request_id: requestId, duration_ms: null, artifacts: [], sources: [], used_context: [], created_at: now, updated_at: now };
}

function subjectLabelFor(kind: string, t: ReturnType<typeof useI18n>["t"]) {
  if (kind === "company") return t("home.subjectCompany");
  if (kind === "investment_system") return t("home.subjectSystem");
  if (kind === "psychology") return t("home.subjectPsychology");
  return t("home.subjectGeneral");
}

async function fileBase64(file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  for (let index = 0; index < bytes.length; index += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
  }
  return window.btoa(binary);
}

function makeId() {
  return `${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 10)}`;
}

function isClientThreadId(value: string | null | undefined) {
  return Boolean(value?.startsWith("client:"));
}
