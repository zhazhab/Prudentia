import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useState
} from "react";

export type Locale = "en" | "zh";

export type TranslationKey = keyof typeof translations.en;

interface I18nContextValue {
  locale: Locale;
  languageTag: string;
  setLocale: (locale: Locale) => void;
  t: (key: TranslationKey, values?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextValue | null>(null);
const storageKey = "prudentia.locale";

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() => initialLocale());

  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
    window.localStorage.setItem(storageKey, locale);
  }, [locale]);

  const value = useMemo<I18nContextValue>(() => {
    const languageTag = locale === "zh" ? "zh-CN" : "en-US";

    return {
      locale,
      languageTag,
      setLocale: setLocaleState,
      t: (key, values) => interpolate(translations[locale][key] ?? translations.en[key], values)
    };
  }, [locale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const value = useContext(I18nContext);
  if (!value) {
    throw new Error("useI18n must be used inside I18nProvider");
  }
  return value;
}

function initialLocale(): Locale {
  const stored = window.localStorage.getItem(storageKey);
  if (stored === "zh" || stored === "en") {
    return stored;
  }

  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

function interpolate(template: string, values: Record<string, string | number> = {}) {
  return Object.entries(values).reduce(
    (result, [key, value]) => result.replaceAll(`{${key}}`, String(value)),
    template
  );
}

const translations = {
  en: {
    "app.subtitle": "Investment OS",
    "app.navLabel": "Main navigation",
    "app.languageLabel": "Language",
    "app.langEnglish": "EN",
    "app.langChinese": "中文",
    "nav.portfolio": "Portfolio",
    "nav.memos": "Memos",
    "nav.settings": "Settings",

    "common.cancel": "Cancel",


    "portfolio.eyebrow": "Portfolio",
    "portfolio.title": "Holdings and weight",
    "portfolio.performance": "Performance",
    "portfolio.performanceSnapshotBasis": "Portfolio return uses trade-adjusted TWR. Holding returns stay broker-style and do not adjust portfolio-level buy/sell changes.",
    "portfolio.performancePeriod": "Performance period",
    "portfolio.performanceView": "Performance view",
    "portfolio.periodMonth": "This month",
    "portfolio.periodYear": "This year",
    "portfolio.periodSinceInception": "Since inception",
    "portfolio.viewAmount": "Amount",
    "portfolio.viewPercent": "Percent",
    "portfolio.periodReturn": "Period return",
    "portfolio.annualizedReturn": "Annualized return",
    "portfolio.annualizedReturnDetail": "Based on the selected snapshot period.",
    "portfolio.loadingPerformance": "Loading...",
    "portfolio.performancePercentDetail": "{value} return",
    "portfolio.performanceAmountDetail": "{value} change",
    "portfolio.performanceTwrPercentDetail": "{value} trade-adjusted P/L; net trade adjustment {flow}; snapshot return {simple}",
    "portfolio.performanceTwrAmountDetail": "{value} TWR; net trade adjustment {flow}; snapshot return {simple}",
    "portfolio.partialPeriod": "Since {date}; no snapshot exists at the period start.",
    "portfolio.benchmarkComparison": "Portfolio vs benchmarks",
    "portfolio.benchmarkProxyNote": "S&P and Hang Seng use ETF proxies; SSE uses the official Composite index.",
    "portfolio.benchmarkMetric": "Benchmark comparison metric",
    "portfolio.benchmarkMetricCumulative": "Return",
    "portfolio.benchmarkMetricAnnualized": "Annualized",
    "portfolio.benchmarkMetricExcess": "Excess",
    "portfolio.benchmarkExcessLabel": "Excess vs {benchmark}",
    "portfolio.performancePortfolio": "Portfolio",
    "portfolio.benchmarkSp500": "S&P proxy",
    "portfolio.benchmarkHangSeng": "Hang Seng proxy",
    "portfolio.benchmarkSse": "SSE Composite",
    "portfolio.benchmarkUnavailable": "unavailable",
    "portfolio.singleSnapshotChartNote": "Only one snapshot is available, so the chart shows the starting point. A line appears after the next snapshot.",
    "portfolio.noPerformanceTitle": "No performance snapshots",
    "portfolio.noPerformanceBody": "Import, edit, delete, or let the daily price job run to create the first snapshot.",
    "portfolio.addFile": "Add file",
    "portfolio.unsupportedImportFile": "Unsupported file: {name}. Add CSV, TSV, XLSX, PNG, JPG, JPEG, or WebP.",
    "portfolio.mixedImportFileTypes": "Add either one CSV/Excel file or screenshots, not both at the same time.",
    "portfolio.oneTabularFileOnly": "Add one CSV/Excel file at a time.",
    "portfolio.addManualRow": "Add row",
    "portfolio.manualEntry": "Manual entry",
    "portfolio.appendFileConfirm": "Append this file to the current draft? Cancel will replace the draft.",
    "portfolio.resolveDraftSymbols": "Match codes",
    "portfolio.symbolResolveDone": "Code matching finished: {count} rows updated.",
    "portfolio.selectColumn": "Select column",
    "portfolio.positions": "Positions",
    "portfolio.refreshPrices": "Refresh prices",
    "portfolio.tableSymbol": "Symbol",
    "portfolio.tableName": "Name",
    "portfolio.tableQty": "Qty",
    "portfolio.tableAvgCost": "Avg cost",
    "portfolio.tableMarketValue": "Market value",
    "portfolio.tablePl": "P/L",
    "portfolio.tablePlPct": "P/L %",
    "portfolio.tablePeriodReturnPct": "Period return %",
    "portfolio.tableWeight": "Weight",
    "portfolio.noPositionsTitle": "No positions tracked",
    "portfolio.noPositionsBody": "Import holdings to connect memos, decisions, and portfolio exposure.",
    "portfolio.mapSymbol": "Symbol",
    "portfolio.mapName": "Name",
    "portfolio.mapQuantity": "Quantity",
    "portfolio.mapAverageCost": "Average cost",
    "portfolio.mapCurrency": "Currency",
    "portfolio.mapAccount": "Account",
    "portfolio.mapMarket": "Market",
    "portfolio.mapSector": "Sector",
    "portfolio.mapImportedMarketValue": "Market value",
    "portfolio.mapNotes": "Notes",
    "portfolio.importTools": "Import tools",
    "portfolio.importToolsBody": "Load a file or screenshot into one editable draft table, then confirm the rows you want to merge into holdings.",
    "portfolio.applyMapping": "Apply mapping",
    "portfolio.preparingDraft": "Preparing import draft...",
    "portfolio.clearDraft": "Clear draft",
    "portfolio.commitDraft": "Confirm draft",
    "portfolio.fixDraftErrors": "Fix or remove rows with blocking errors before confirming.",
    "portfolio.emptyDraftTitle": "No draft rows",
    "portfolio.emptyDraftBody": "Prepare editable holdings from a file, screenshot, or manual row.",
    "portfolio.actions": "Actions",
    "portfolio.imageRowsRecognized": "{count} rows",
    "portfolio.imageStatusQueued": "Queued",
    "portfolio.imageStatusRunning": "Running",
    "portfolio.imageStatusCompleted": "Completed",
    "portfolio.imageStatusFailed": "Failed",
    "portfolio.imageStatusCanceled": "Canceled",
    "portfolio.imageStageAccepted": "Accepted",
    "portfolio.imageStageValidating": "Validating image",
    "portfolio.imageStageUploading": "Preparing image",
    "portfolio.imageStageRecognizing": "Recognizing screenshot",
    "portfolio.imageStageNormalizing": "Preparing rows",
    "portfolio.imageStageResolvingSymbols": "Matching codes",
    "portfolio.cnyTotal": "CNY total",
    "portfolio.cnyPl": "CNY P/L",
    "portfolio.editPosition": "Edit position",
    "portfolio.savePosition": "Save position",


    "memos.eyebrow": "Memos",
    "memos.title": "Thesis, risks, catalysts, and kill criteria",
    "memos.newMemo": "New memo",
    "memos.fieldTitle": "Title",
    "memos.titlePlaceholder": "Company or idea",
    "memos.fieldSymbol": "Symbol",
    "memos.fieldNotes": "Raw notes",
    "memos.notesPlaceholder": "Paste observations, numbers, links, questions, or a rough thesis.",
    "memos.fieldTags": "Tags",
    "memos.tagsPlaceholder": "quality, compounder",
    "memos.create": "Create memo",
    "memos.library": "Memo library",
    "memos.noMemosTitle": "No memos yet",
    "memos.noMemosBody": "Start with rough notes; the AI provider can extract a structured memo from them.",
    "memos.noSymbol": "No symbol",
    "memos.aiExtract": "AI extract",
    "memos.notes": "Notes",
    "memos.noNotes": "No notes yet",
    "memos.thesis": "Thesis",
    "memos.risks": "Risks",
    "memos.catalysts": "Catalysts",
    "memos.disconfirmingEvidence": "Disconfirming evidence",
    "memos.aiChecklist": "AI checklist",




    "settings.eyebrow": "Settings",
    "settings.title": "AI provider and credential setup",
    "settings.provider": "Provider",
    "settings.providerMock": "Mock",
    "settings.providerOpenai": "OpenAI-compatible",
    "settings.providerCli": "CLI provider",
    "settings.openaiBaseUrl": "OpenAI-compatible base URL",
    "settings.openaiModel": "OpenAI-compatible model",
    "settings.openaiApiKey": "API key",
    "settings.openaiApiKeyPlaceholder": "Leave blank to keep current key",
    "settings.keyConfigured": "A key is currently configured.",
    "settings.keyMissing": "No API key is configured.",
    "settings.cliPath": "CLI path",
    "settings.cliModel": "CLI model override",
    "settings.cliProfile": "CLI profile",
    "settings.cliAdvanced": "Advanced CLI options",
    "settings.cliLoginCommand": "CLI login command",
    "settings.cliHelp": "For Codex, run this in a terminal once, finish the browser/device-code flow, then choose CLI as the provider.",
    "settings.localSaveNote": "Saving writes these settings to the local .env file and applies them immediately.",
    "settings.save": "Save AI settings",
    "settings.saved": "Settings saved",
    "settings.mockNote": "Mock mode stays fully local and deterministic.",
    "settings.openaiNote": "Use an OpenAI-compatible endpoint when you want hosted model calls through the backend.",
    "settings.cliNote": "Use the Codex CLI provider when you want Prudentia to reuse your local device-code login.",
  },
  zh: {
    "app.subtitle": "投资操作系统",
    "app.navLabel": "主导航",
    "app.languageLabel": "语言",
    "app.langEnglish": "EN",
    "app.langChinese": "中文",
    "nav.portfolio": "组合",
    "nav.memos": "备忘录",
    "nav.settings": "设置",

    "common.cancel": "取消",


    "portfolio.eyebrow": "Portfolio",
    "portfolio.title": "持仓与权重",
    "portfolio.performance": "收益表现",
    "portfolio.performanceSnapshotBasis": "组合收益率使用已调整买入/卖出变动的时间加权收益率；单只持仓收益率保持券商浮盈亏口径。",
    "portfolio.performancePeriod": "收益周期",
    "portfolio.performanceView": "收益视角",
    "portfolio.periodMonth": "本月",
    "portfolio.periodYear": "本年",
    "portfolio.periodSinceInception": "记录起",
    "portfolio.viewAmount": "金额",
    "portfolio.viewPercent": "百分比",
    "portfolio.periodReturn": "周期收益",
    "portfolio.annualizedReturn": "年化收益",
    "portfolio.annualizedReturnDetail": "基于当前选择周期的快照计算。",
    "portfolio.loadingPerformance": "读取中...",
    "portfolio.performancePercentDetail": "{value} 收益率",
    "portfolio.performanceAmountDetail": "{value} 变动",
    "portfolio.performanceTwrPercentDetail": "{value} 已扣除买入/卖出变动的盈亏；净交易调整 {flow}；未调整快照收益 {simple}",
    "portfolio.performanceTwrAmountDetail": "{value} 时间加权收益率；净交易调整 {flow}；未调整快照收益 {simple}",
    "portfolio.partialPeriod": "自 {date} 起；周期起点没有快照。",
    "portfolio.benchmarkComparison": "组合 vs 基准指数",
    "portfolio.benchmarkProxyNote": "标普和恒生使用 ETF 代理；上证使用官方上证综指。",
    "portfolio.benchmarkMetric": "指数对比维度",
    "portfolio.benchmarkMetricCumulative": "累计收益",
    "portfolio.benchmarkMetricAnnualized": "年化收益",
    "portfolio.benchmarkMetricExcess": "超额收益",
    "portfolio.benchmarkExcessLabel": "相对{benchmark}",
    "portfolio.performancePortfolio": "组合",
    "portfolio.benchmarkSp500": "标普代理",
    "portfolio.benchmarkHangSeng": "恒生代理",
    "portfolio.benchmarkSse": "上证综指",
    "portfolio.benchmarkUnavailable": "不可用",
    "portfolio.singleSnapshotChartNote": "当前只有一条快照，图中先显示起点；下一条快照生成后会形成折线。",
    "portfolio.noPerformanceTitle": "暂无收益快照",
    "portfolio.noPerformanceBody": "导入、编辑、删除或等待每日行情任务运行后会生成第一条快照。",
    "portfolio.addFile": "新增文件",
    "portfolio.unsupportedImportFile": "不支持的文件：{name}。请新增 CSV、TSV、XLSX、PNG、JPG、JPEG 或 WebP。",
    "portfolio.mixedImportFileTypes": "请一次只新增一种文件：一个 CSV/Excel 文件，或一组截图。",
    "portfolio.oneTabularFileOnly": "CSV/Excel 文件请一次只新增一个。",
    "portfolio.addManualRow": "新增行",
    "portfolio.manualEntry": "手动录入",
    "portfolio.appendFileConfirm": "将该文件追加到当前草稿？取消则替换当前草稿。",
    "portfolio.resolveDraftSymbols": "匹配代码",
    "portfolio.symbolResolveDone": "代码匹配完成：更新 {count} 行。",
    "portfolio.selectColumn": "选择列",
    "portfolio.positions": "持仓",
    "portfolio.refreshPrices": "刷新行情",
    "portfolio.tableSymbol": "代码",
    "portfolio.tableName": "名称",
    "portfolio.tableQty": "数量",
    "portfolio.tableAvgCost": "平均成本",
    "portfolio.tableMarketValue": "市值",
    "portfolio.tablePl": "盈亏",
    "portfolio.tablePlPct": "收益率",
    "portfolio.tablePeriodReturnPct": "周期收益率",
    "portfolio.tableWeight": "权重",
    "portfolio.noPositionsTitle": "还没有跟踪持仓",
    "portfolio.noPositionsBody": "导入持仓，把备忘录、决策和组合暴露连接起来。",
    "portfolio.mapSymbol": "代码",
    "portfolio.mapName": "名称",
    "portfolio.mapQuantity": "数量",
    "portfolio.mapAverageCost": "平均成本",
    "portfolio.mapCurrency": "币种",
    "portfolio.mapAccount": "账户",
    "portfolio.mapMarket": "市场",
    "portfolio.mapSector": "行业",
    "portfolio.mapImportedMarketValue": "市值",
    "portfolio.mapNotes": "备注",
    "portfolio.importTools": "导入工具",
    "portfolio.importToolsBody": "将文件或截图先转成同一张可编辑草稿表，确认后再合并写入持仓。",
    "portfolio.applyMapping": "应用映射",
    "portfolio.preparingDraft": "正在准备导入草稿...",
    "portfolio.clearDraft": "清除草稿",
    "portfolio.commitDraft": "确认草稿",
    "portfolio.fixDraftErrors": "请先修正或删除有阻断错误的行。",
    "portfolio.emptyDraftTitle": "暂无草稿行",
    "portfolio.emptyDraftBody": "通过文件、截图或手填行生成可编辑持仓草稿。",
    "portfolio.actions": "操作",
    "portfolio.imageRowsRecognized": "{count} 行",
    "portfolio.imageStatusQueued": "排队中",
    "portfolio.imageStatusRunning": "运行中",
    "portfolio.imageStatusCompleted": "已完成",
    "portfolio.imageStatusFailed": "失败",
    "portfolio.imageStatusCanceled": "已取消",
    "portfolio.imageStageAccepted": "已接收",
    "portfolio.imageStageValidating": "正在校验图片",
    "portfolio.imageStageUploading": "正在准备图片",
    "portfolio.imageStageRecognizing": "正在识别截图",
    "portfolio.imageStageNormalizing": "正在整理草稿行",
    "portfolio.imageStageResolvingSymbols": "正在匹配代码",
    "portfolio.cnyTotal": "人民币总额",
    "portfolio.cnyPl": "人民币盈亏",
    "portfolio.editPosition": "编辑持仓",
    "portfolio.savePosition": "保存持仓",


    "memos.eyebrow": "备忘录",
    "memos.title": "Thesis、风险、催化剂和退出条件",
    "memos.newMemo": "新建备忘录",
    "memos.fieldTitle": "标题",
    "memos.titlePlaceholder": "公司或投资想法",
    "memos.fieldSymbol": "代码",
    "memos.fieldNotes": "原始笔记",
    "memos.notesPlaceholder": "粘贴观察、数据、链接、问题或粗略 thesis。",
    "memos.fieldTags": "标签",
    "memos.tagsPlaceholder": "质量, 复利",
    "memos.create": "创建备忘录",
    "memos.library": "备忘录库",
    "memos.noMemosTitle": "还没有备忘录",
    "memos.noMemosBody": "从粗略笔记开始，AI provider 可以提炼出结构化备忘录。",
    "memos.noSymbol": "无代码",
    "memos.aiExtract": "AI 提炼",
    "memos.notes": "笔记",
    "memos.noNotes": "暂无笔记",
    "memos.thesis": "Thesis",
    "memos.risks": "风险",
    "memos.catalysts": "催化剂",
    "memos.disconfirmingEvidence": "反证条件",
    "memos.aiChecklist": "AI 检查清单",




    "settings.eyebrow": "设置",
    "settings.title": "AI provider 与凭据配置",
    "settings.provider": "Provider",
    "settings.providerMock": "Mock",
    "settings.providerOpenai": "OpenAI-compatible",
    "settings.providerCli": "CLI provider",
    "settings.openaiBaseUrl": "OpenAI-compatible Base URL",
    "settings.openaiModel": "OpenAI-compatible 模型",
    "settings.openaiApiKey": "API Key",
    "settings.openaiApiKeyPlaceholder": "留空则保留当前 key",
    "settings.keyConfigured": "当前已配置 key。",
    "settings.keyMissing": "尚未配置 API key。",
    "settings.cliPath": "CLI 路径",
    "settings.cliModel": "CLI 模型覆盖",
    "settings.cliProfile": "CLI profile",
    "settings.cliAdvanced": "CLI 高级选项",
    "settings.cliLoginCommand": "CLI 登录命令",
    "settings.cliHelp": "如果使用 Codex，先在终端运行一次该命令，完成浏览器/device-code 登录，再选择 CLI provider。",
    "settings.localSaveNote": "保存会写入本地 .env，并立即应用到当前后端运行时。",
    "settings.save": "保存 AI 设置",
    "settings.saved": "设置已保存",
    "settings.mockNote": "Mock 模式完全本地、确定性输出。",
    "settings.openaiNote": "选择 OpenAI-compatible 后，后端会通过兼容接口调用托管模型。",
    "settings.cliNote": "选择 CLI provider 后，Prudentia 会复用你本机 Codex CLI 的 device-code 登录。"
  }
} as const;
