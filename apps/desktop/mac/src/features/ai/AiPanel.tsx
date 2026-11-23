import { Fragment, useEffect, useReducer, useRef, useState } from 'react';
import type { FormEvent } from 'react';
import { AlertTriangle, CheckCircle2, ChevronDown, Clock3, Copy, CreditCard, LockKeyhole, Play, Send, Server, ShieldCheck, Sparkles, Square } from 'lucide-react';
import { desktop, isDesktop } from '../../ipc/client';
import type { MessageKey } from '../../i18n';
import type { AiContextData, AiContextPermissions, AiConversationMessage, AiEntitlement, ExecutionMode, ExecutionRecord, SessionSnapshot, TargetRef } from '../../types';
import { AiBillingDialog } from './AiBillingDialog';
import { aiReducer, initialAiState } from './aiSession';
import { AiConversationCache } from './aiConversationCache';
import type { AnalysisState } from './aiConversationCache';
import { runAiPlan } from './aiAgent';
import type { AiAgentOptions } from './aiAgent';
import { runDynamicAgent } from './dynamicAiAgent';
import type { DynamicAgentSession, DynamicAgentState } from './dynamicAiAgent';
import { selectAuthorizedContext } from './context';

type Translate = (key: MessageKey) => string;
type ContextCategory = 'serverLabel' | 'systemInfo' | 'workingDirectory' | 'selectedOutput' | 'recentResults' | 'directoryEntries';

const contextCategories: ContextCategory[] = ['systemInfo', 'workingDirectory', 'selectedOutput', 'recentResults', 'directoryEntries', 'serverLabel'];
const defaultPermissions: Record<ContextCategory, boolean> = { serverLabel: true, systemInfo: true, workingDirectory: true, selectedOutput: true, recentResults: true, directoryEntries: true };
const contextStorageKey = import.meta.env.DEV ? 'sparrowshell.ai.context.development.v2' : 'sparrowshell.ai.context.v2';
const sameTarget = (left: TargetRef | null, right: TargetRef) => left?.serverId === right.serverId && left.sessionId === right.sessionId && left.generation === right.generation;

export function AiPanel({ session, serverName, connected, locale, contextData, maxSteps, t }: { session: SessionSnapshot | null; serverName: string; connected: boolean; locale: 'zh' | 'en'; contextData?: AiContextData; maxSteps: number; t: Translate }) {
  const [state, dispatch] = useReducer(aiReducer, initialAiState);
  const [input, setInput] = useState('');
  const [modeOpen, setModeOpen] = useState(false);
  const [contextOpen, setContextOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [safeStopReason, setSafeStopReason] = useState<DynamicAgentState | null>(null);
  const [entitlement, setEntitlement] = useState<AiEntitlement | null>(null);
  const [billingOpen, setBillingOpen] = useState(false);
  const [analyses, setAnalyses] = useState<Record<string, AnalysisState>>({});
  const [permissions, setPermissions] = useState<AiContextPermissions>(() => {
    try { return { ...defaultPermissions, ...JSON.parse(localStorage.getItem(contextStorageKey) ?? '{}') }; } catch { return defaultPermissions; }
  });
  const target = session?.target ?? null;
  const currentTarget = useRef(target);
  const stateRef = useRef(state);
  const conversationCacheRef = useRef(new AiConversationCache());
  const executionsRef = useRef<Record<string, ExecutionRecord>>({});
  const analysesRef = useRef<Record<string, AnalysisState>>({});
  const permissionsRef = useRef(permissions);
  const operationRef = useRef(0);
  const dynamicSessionRef = useRef<DynamicAgentSession | null>(null);
  const dynamicHistoryRef = useRef<AiConversationMessage[]>([]);
  const qwenSessionIdRef = useRef<string | null>(null);
  const safeStopReasonRef = useRef<DynamicAgentState | null>(null);
  const modePickerRef = useRef<HTMLDivElement>(null);
  currentTarget.current = target;
  stateRef.current = state;
  permissionsRef.current = permissions;
  safeStopReasonRef.current = safeStopReason;

  const refreshEntitlement = async () => {
    if (!isDesktop) return;
    try { setEntitlement(await desktop.aiEntitlement()); } catch { /* Keep the previous quota display during a temporary network failure. */ }
  };
  const releaseTask = async (taskId: string) => {
    if (!isDesktop) return;
    try { await desktop.aiReleaseTask(taskId); } catch { /* The server-side reservation TTL is the fallback. */ }
    await refreshEntitlement();
  };
  useEffect(() => { void refreshEntitlement(); }, []);

  useEffect(() => {
    operationRef.current += 1;
    const cached = target ? conversationCacheRef.current.read(target) : undefined;
    const restoredDynamicSession = cached?.dynamicSession && target && sameTarget(cached.dynamicSession.target, target)
      ? cached.dynamicSession
      : null;
    dispatch({ type: 'target_changed', target, conversation: cached?.conversation });
    executionsRef.current = cached?.executions ?? {};
    analysesRef.current = cached?.analyses ?? {};
    dynamicSessionRef.current = restoredDynamicSession;
    dynamicHistoryRef.current = cached?.dynamicHistory ?? [];
    qwenSessionIdRef.current = cached?.qwenSessionId ?? null;
    setAnalyses(cached?.analyses ?? {});
    setSafeStopReason(restoredDynamicSession ? cached?.safeStopReason ?? null : null);
    setBusy(false);
    return () => {
      if (!target) return;
      const currentState = stateRef.current;
      const executions = executionsRef.current;
      conversationCacheRef.current.write(target, {
        conversation: {
          messages: currentState.messages,
          plans: currentState.plans,
          executions,
          error: currentState.error,
        },
        executions,
        analyses: Object.fromEntries(Object.entries(analysesRef.current).filter(([, analysis]) => analysis.status !== 'loading')),
        dynamicSession: dynamicSessionRef.current,
        dynamicHistory: dynamicHistoryRef.current,
        qwenSessionId: qwenSessionIdRef.current,
        safeStopReason: safeStopReasonRef.current,
      });
    };
  }, [target?.serverId, target?.sessionId, target?.generation]);
  useEffect(() => { localStorage.setItem(contextStorageKey, JSON.stringify(permissions)); }, [permissions]);
  useEffect(() => {
    if (!modeOpen) return;
    const closeOnOutsideClick = (event: MouseEvent) => {
      if (!(event.target instanceof Node) || !modePickerRef.current?.contains(event.target)) setModeOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => { if (event.key === 'Escape') setModeOpen(false); };
    document.addEventListener('mousedown', closeOnOutsideClick);
    document.addEventListener('keydown', closeOnEscape);
    return () => {
      document.removeEventListener('mousedown', closeOnOutsideClick);
      document.removeEventListener('keydown', closeOnEscape);
    };
  }, [modeOpen]);

  const updateAnalysis = (executionId: string, analysis: AnalysisState) => {
    analysesRef.current = { ...analysesRef.current, [executionId]: analysis };
    setAnalyses(analysesRef.current);
  };
  const receiveAnalysis = (executionId: string, text: string, requestTarget: TargetRef) => {
    updateAnalysis(executionId, { status: 'success', text });
    dispatch({ type: 'analysis_received', target: requestTarget, message: { id: crypto.randomUUID(), role: 'assistant', kind: 'analysis', text } });
  };
  const agentOptions = (requestTarget: TargetRef, requestMode: ExecutionMode): Omit<AiAgentOptions, 'plan'> => ({
    target: requestTarget,
    mode: requestMode,
    runNext: desktop.aiRunNext,
    isCurrent: () => sameTarget(currentTarget.current, requestTarget),
    onProgress: (progress) => {
      if (progress.execution) executionsRef.current = { ...executionsRef.current, [progress.execution.executionId]: progress.execution };
      dispatch({ type: 'progress_received', progress });
    },
    getExecution: (executionId) => executionsRef.current[executionId],
    shouldAnalyze: (executionId) => !analysesRef.current[executionId],
    canAnalyze: () => permissionsRef.current.recentResults,
    onAnalysisStart: (executionId) => updateAnalysis(executionId, { status: 'loading', text: t('analyzing') }),
    onAnalysis: (executionId, text) => receiveAnalysis(executionId, text, requestTarget),
    onAnalysisError: (executionId, message) => updateAnalysis(executionId, { status: 'error', text: message }),
    onAnalysisDenied: (executionId) => updateAnalysis(executionId, { status: 'error', text: t('analysisNeedsPermission') }),
    analyze: (executionId) => desktop.aiAnalyze(executionId, locale, requestTarget, true),
  });
  const executePlan = (plan: NonNullable<typeof state.plan>, requestTarget: TargetRef, requestMode: ExecutionMode, initialProgress?: Parameters<typeof runAiPlan>[1]) =>
    runAiPlan({ ...agentOptions(requestTarget, requestMode), plan }, initialProgress);
  const isCurrentOperation = (operation: number, requestTarget: TargetRef) => operationRef.current === operation && sameTarget(currentTarget.current, requestTarget);
  const showOperationError = (operation: number, requestTarget: TargetRef, reason: unknown) => {
    const message = reason instanceof Error ? reason.message : String(reason);
    if (message.includes('AI 次数已用完')) setBillingOpen(true);
    if (isCurrentOperation(operation, requestTarget)) dispatch({ type: 'provider_failed', message: message === 'approval has expired' ? t('approvalExpired') : message });
  };
  const executeDynamic = async (
    operation: number,
    text: string,
    requestTarget: TargetRef,
    requestMode: ExecutionMode,
    history: AiConversationMessage[],
    session?: DynamicAgentSession,
    initialProgress?: Parameters<typeof runDynamicAgent>[1],
  ) => {
    const currentTaskId = session?.taskId ?? crypto.randomUUID();
    let result;
    try {
      result = await runDynamicAgent({
        intent: text,
        taskId: currentTaskId,
        target: requestTarget,
        mode: requestMode,
        session,
        maxSteps,
        isCurrent: () => isCurrentOperation(operation, requestTarget),
        canSendResults: () => permissionsRef.current.recentResults,
        requestTurn: async (executionIds) => {
          const currentPermissions = permissionsRef.current;
          const inputContext = selectAuthorizedContext({ ...contextData, serverLabel: serverName, recentResults: [] }, currentPermissions);
          const response = await desktop.aiAgentTurn(text, currentTaskId, history, locale, requestTarget, inputContext, currentPermissions, executionIds, qwenSessionIdRef.current);
          if (isCurrentOperation(operation, requestTarget)) qwenSessionIdRef.current = response.sessionId;
          return response;
        },
        runPlan: (plan) => runAiPlan({
          ...agentOptions(requestTarget, requestMode),
          plan,
          analyzeResults: false,
          isCurrent: () => isCurrentOperation(operation, requestTarget),
        }),
        onPlan: (summary, plan) => {
          if (isCurrentOperation(operation, requestTarget)) {
            dispatch({ type: 'provider_plan_received', message: { id: crypto.randomUUID(), role: 'assistant', kind: 'plan', text: summary }, plan });
          }
        },
        onFinalAnswer: (answer) => {
          if (isCurrentOperation(operation, requestTarget)) {
            dispatch({ type: 'analysis_received', target: requestTarget, message: { id: crypto.randomUUID(), role: 'assistant', kind: 'analysis', text: answer } });
          }
        },
      }, initialProgress);
    } catch (reason) {
      await releaseTask(currentTaskId);
      throw reason;
    }
    if (result.state === 'final_answer') await refreshEntitlement();
    if (['permission_required', 'step_limit', 'repeated_command', 'stopped', 'result_unknown', 'stale'].includes(result.state)) {
      await releaseTask(currentTaskId);
    }
    if (!isCurrentOperation(operation, requestTarget)) return result;
    dynamicSessionRef.current = result.session;
    const safeStop = ['permission_required', 'step_limit', 'repeated_command'].includes(result.state) ? result.state : null;
    setSafeStopReason(safeStop);
    if (result.state === 'permission_required') {
      dispatch({ type: 'provider_failed', message: t('analysisNeedsPermission') });
      setContextOpen(true);
    } else if (result.state === 'step_limit') {
      dispatch({ type: 'provider_failed', message: t('agentStepLimit').replace('{limit}', String(maxSteps)) });
    } else if (result.state === 'repeated_command') {
      dispatch({ type: 'provider_failed', message: t('agentRepeatedCommand') });
    }
    return result;
  };

  const modeKey = state.mode === 'confirm_each' ? 'confirmEach' : state.mode === 'auto_all' ? 'autoAll' : 'autoExcept';
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const text = input.trim();
    if (!text || !target || !connected || busy) return;
    const requestTarget = target;
    const requestMode = state.mode;
    const operation = ++operationRef.current;
    const history = state.messages
      .filter((message) => message.kind !== 'plan')
      .slice(-12)
      .map((message) => ({ role: message.role, content: message.text }));
    dynamicSessionRef.current = null;
    dynamicHistoryRef.current = history;
    executionsRef.current = {};
    analysesRef.current = {};
    setAnalyses({});
    setSafeStopReason(null);
    dispatch({ type: 'prompt_sent', id: crypto.randomUUID(), text, target: requestTarget });
    setInput('');
    setBusy(true);
    void (async () => {
      try {
        await executeDynamic(operation, text, requestTarget, requestMode, history);
      } catch (reason) {
        showOperationError(operation, requestTarget, reason);
      } finally {
        if (operationRef.current === operation) setBusy(false);
      }
    })();
  };
  const runNext = async () => {
    if (!state.plan || !target || busy) return;
    const requestTarget = target;
    const operation = ++operationRef.current;
    setBusy(true);
    try {
      await executePlan(state.plan, requestTarget, state.mode);
    } catch (reason) {
      showOperationError(operation, requestTarget, reason);
    } finally {
      if (operationRef.current === operation) setBusy(false);
    }
  };
  const resumeDynamic = async () => {
    if (!dynamicSessionRef.current || !target || busy) return;
    const requestTarget = target;
    const operation = ++operationRef.current;
    setBusy(true);
    setSafeStopReason(null);
    try {
      const session = dynamicSessionRef.current;
      await executeDynamic(operation, session.intent, requestTarget, state.mode, dynamicHistoryRef.current, session);
    } catch (reason) {
      showOperationError(operation, requestTarget, reason);
    } finally {
      if (operationRef.current === operation) setBusy(false);
    }
  };
  const approve = async () => {
    if (!state.plan || !state.pendingApproval || !target || busy) return;
    const requestTarget = target;
    const operation = ++operationRef.current;
    setBusy(true);
    try {
      const approval = state.pendingApproval;
      const currentPlan = state.plan;
      const progress = await desktop.aiApproveAndRun(currentPlan.planId, approval.approvalId, approval.command, requestTarget);
      if (!isCurrentOperation(operation, requestTarget)) return;
      dispatch({ type: 'progress_received', progress });
      const dynamicSession = dynamicSessionRef.current;
      if (dynamicSession) {
        await executeDynamic(operation, dynamicSession.intent, requestTarget, state.mode, dynamicHistoryRef.current, dynamicSession, progress);
      } else {
        await executePlan(currentPlan, requestTarget, state.mode, progress);
      }
    } catch (reason) {
      showOperationError(operation, requestTarget, reason);
    } finally {
      if (operationRef.current === operation) setBusy(false);
    }
  };
  const stop = async () => {
    if (!target || (!state.plan && !busy)) return;
    const requestTarget = target;
    const currentPlan = state.plan;
    const dynamicTaskId = dynamicSessionRef.current?.taskId;
    const operation = ++operationRef.current;
    setBusy(true);
    try {
      const plan = currentPlan ? await desktop.aiStop(currentPlan.planId) : null;
      if (dynamicTaskId) await releaseTask(dynamicTaskId);
      if (!isCurrentOperation(operation, requestTarget)) return;
      if (plan && dynamicSessionRef.current) dynamicSessionRef.current = { ...dynamicSessionRef.current, plan };
      setSafeStopReason('stopped');
      if (plan) dispatch({ type: 'progress_received', progress: { plan, approval: null, execution: null } });
      else dispatch({ type: 'provider_failed', message: '' });
    } catch (reason) {
      if (dynamicTaskId) await releaseTask(dynamicTaskId);
      showOperationError(operation, requestTarget, reason);
    } finally {
      if (operationRef.current === operation) setBusy(false);
    }
  };

  const analyze = async (executionId: string) => {
    if (!target || analysesRef.current[executionId]?.status === 'loading') return;
    const requestTarget = target;
    if (!permissionsRef.current.recentResults) {
      updateAnalysis(executionId, { status: 'error', text: t('analysisNeedsPermission') });
      return;
    }
    updateAnalysis(executionId, { status: 'loading', text: t('analyzing') });
    try {
      const response = await desktop.aiAnalyze(executionId, locale, requestTarget, true);
      if (!sameTarget(currentTarget.current, requestTarget)) return;
      receiveAnalysis(executionId, response.analysis, requestTarget);
    } catch (reason) {
      if (!sameTarget(currentTarget.current, requestTarget)) return;
      updateAnalysis(executionId, { status: 'error', text: reason instanceof Error ? reason.message : String(reason) });
    }
  };

  return <section className={`ai-panel ${state.mode === 'auto_all' ? 'full-auto-active' : ''}`}>
    <div className="ai-caption">
      <div className="ai-title"><Sparkles size={14} /><strong>{t('aiAssistant')}</strong></div>
      <div className="ai-header-actions">
        <div className="ai-quota-actions">
          <button className="ai-quota-display" onClick={() => setBillingOpen(true)} title={locale === 'zh' ? '查看 AI 额度与套餐' : 'View AI quota and plans'}><span>{locale === 'zh' ? '剩余' : 'Left'}</span><strong>{entitlement?.remaining ?? '—'}</strong><span>{locale === 'zh' ? '次' : 'tasks'}</span></button>
          <button className="ai-recharge-button" onClick={() => setBillingOpen(true)}><CreditCard size={12} />{locale === 'zh' ? '充值' : 'Top up'}</button>
        </div>
        <div className="mode-picker" ref={modePickerRef}>
          <button className={`mode-button ${state.mode}`} aria-expanded={modeOpen} onClick={() => setModeOpen((open) => !open)}><ShieldCheck size={13} /><span>{t(modeKey)}</span><ChevronDown size={12} /></button>
          {modeOpen && <div className="mode-menu" role="menu" aria-label={t('commandMode')}>
            {(['confirm_each', 'auto_except_high_risk', 'auto_all'] as ExecutionMode[]).map((mode) => {
              const key = mode === 'confirm_each' ? 'confirmEach' : mode === 'auto_all' ? 'autoAll' : 'autoExcept';
              const detail = mode === 'confirm_each' ? 'confirmEachDetail' : mode === 'auto_all' ? 'autoAllDetail' : 'autoExceptDetail';
              return <button role="menuitem" key={mode} className={state.mode === mode ? 'selected' : ''} onClick={() => { dispatch({ type: 'mode_changed', mode }); setModeOpen(false); }}><span><strong>{t(key)}</strong><small>{t(detail)}</small></span>{state.mode === mode && <CheckCircle2 size={14} />}</button>;
            })}
          </div>}
        </div>
      </div>
    </div>
    {state.mode === 'auto_all' && <div className="auto-all-warning"><AlertTriangle size={12} />{t('autoAllWarning')}</div>}
    <div className="ai-context-bar"><span><Server size={12} />{serverName || t('disconnected')}</span><button onClick={() => setContextOpen((open) => !open)}><ShieldCheck size={12} />{t('contextPermissions')}</button></div>
    {contextOpen && <div className="context-permissions"><p>{t('contextDefault')}</p><div>{contextCategories.map((category) => <label key={category}><input type="checkbox" checked={permissions[category]} onChange={(event) => setPermissions((current) => ({ ...current, [category]: event.target.checked }))} /><span>{t(category)}</span></label>)}</div><small>{t('contextNeverSecrets')}</small></div>}
    {!connected && <div className="ai-service-notice"><AlertTriangle size={13} /><span>{t('aiConnectFirst')}</span></div>}
    <div className="ai-conversation">
      {state.messages.length === 0 && !state.plan && <div className="ai-empty"><Sparkles size={21} /><p>{t('noAiMessages')}</p></div>}
      {state.messages.map((message) => {
        const messagePlan = message.planId ? state.plans.find((plan) => plan.planId === message.planId) : null;
        return <Fragment key={message.id}>
          <div className={`ai-message ${message.role} ${message.kind === 'analysis' ? 'analysis-answer' : ''}`}><div className="message-content"><p>{message.text}</p></div></div>
          {messagePlan && <div className="ai-plan">{messagePlan.steps.map((step, index) => {
            const execution = step.executionId ? state.executions[step.executionId] : undefined;
            const approval = state.pendingApproval?.approvalId === step.approvalId ? state.pendingApproval : null;
            const risk = execution?.risk ?? approval?.risk;
            return <div className={`command-card ${risk?.level === 'high' ? 'high-risk' : ''}`} key={`${messagePlan.planId}-${index}-${step.proposal.command}`}>
              <div className="command-card-head"><span>{risk?.level === 'high' ? t('highRisk') : risk?.level === 'unknown' ? t('unknownRisk') : t('suggestedCommand')}</span><button className="icon small" title={t('copyCommand')} onClick={() => void navigator.clipboard.writeText(step.proposal.command)}><Copy size={12} /></button></div>
              <code>{step.proposal.command}</code>
              {approval && <div className="command-review"><span><Clock3 size={12} />{t('waitingApproval')}</span><button className="button primary small" disabled={busy || !connected} onClick={() => void approve()}><Play size={12} />{t('approveRun')}</button></div>}
              {execution && <>
                <div className={`command-result ${execution.state}`}><div><span>{t('executionResult')} · {t(execution.state)}</span><span className="result-tools"><span>{t('exitCode')} {execution.exitStatus ?? t('noExitCode')}</span>{['succeeded', 'failed'].includes(execution.state) && <button className="analysis-button" disabled={!isDesktop || analyses[execution.executionId]?.status === 'loading'} onClick={() => void analyze(execution.executionId)}><Sparkles size={11} />{analyses[execution.executionId]?.status === 'loading' ? t('analyzing') : t('analyzeResult')}</button>}</span></div>{execution.stdout && <pre>{execution.stdout}</pre>}{execution.stderr && <pre className="stderr">{execution.stderr}</pre>}{execution.errorCode && <small>{execution.errorCode}</small>}</div>
                {analyses[execution.executionId] && analyses[execution.executionId].status !== 'loading' && <div className={`command-analysis ${analyses[execution.executionId].status}`} role={analyses[execution.executionId].status === 'error' ? 'alert' : undefined}><Sparkles size={12} /><div><p>{analyses[execution.executionId].text}</p>{analyses[execution.executionId].status === 'success' && <small>{t('analysisReadOnly')}</small>}</div></div>}
              </>}
            </div>;
          })}</div>}
        </Fragment>;
      })}
      {busy && <div className="ai-working" role="status" aria-live="polite" ref={(element) => element?.scrollIntoView({ block: 'nearest' })}><span className="ai-working-spinner" aria-hidden="true" /><span>{t('aiWorking')}</span></div>}
    </div>
    {state.error && <div className="ai-service-notice" role="alert"><AlertTriangle size={13} /><span>{state.error}</span>{safeStopReason === 'permission_required' && <button className="button secondary small" disabled={busy || !permissions.recentResults} onClick={() => void resumeDynamic()}>{t('continueAgent')}</button>}<button type="button" className="icon small ai-notice-close" aria-label={t('dismissNotice')} title={t('dismissNotice')} onClick={() => dispatch({ type: 'provider_failed', message: '' })}>×</button></div>}
    <div className="ai-actions">{state.plan?.state === 'ready' && <button className="button secondary small" disabled={busy || !connected || !isDesktop} onClick={() => void runNext()}><Play size={12} />{t('runNext')}</button>}{(busy || (state.plan && !['completed', 'failed', 'stopped'].includes(state.plan.state))) && <button className="button secondary small" disabled={state.plan?.state === 'stopping'} onClick={() => void stop()}><Square size={11} />{t('stopNext')}</button>}</div>
    <form className="ai-input-wrap" onSubmit={submit}>
      <textarea aria-label={t('aiInputLabel')} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={input} onChange={(event) => setInput(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit(event); } }} placeholder={t('aiPlaceholder')} rows={2} />
      <div className="ai-input-bottom"><span><LockKeyhole size={12} />{t('onlyTarget')} {serverName || '—'}</span><button className="send-button" aria-label={t('send')} disabled={!input.trim() || !connected || busy || !isDesktop}><Send size={13} /></button></div>
    </form>
    {billingOpen && <AiBillingDialog locale={locale} entitlement={entitlement} onClose={() => { setBillingOpen(false); void refreshEntitlement(); }} onChanged={setEntitlement} />}
  </section>;
}
