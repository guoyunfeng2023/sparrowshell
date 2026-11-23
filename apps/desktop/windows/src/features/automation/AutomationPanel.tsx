import { useEffect, useMemo, useState } from 'react';
import { AlertTriangle, ChevronDown, ChevronUp, History, Play, RefreshCw, Save, Square, Trash2, Workflow, X } from 'lucide-react';
import { desktop } from '../../ipc/client';
import { buildBatchSpec } from './batchSpec';
import { summarizeBatchProgress } from './batchProgress';
import type {
  AutomationDefinition, AutomationDefinitionKind, AutomationRunRecord, BatchRun, BatchState, ExecutionMode,
  ServerRecord, SessionSnapshot, TargetRef, TargetRunState, WorkflowExecution, WorkflowVersion,
} from '../../types';

interface ConnectedTarget { server: ServerRecord; session: SessionSnapshot }
interface Props { targets: ConnectedTarget[]; locale: 'zh' | 'en'; onClose: () => void }
type Tab = 'batch' | 'workflow' | 'templates' | 'history';
type DefinitionItem = AutomationDefinition & { kind: AutomationDefinitionKind };

const text = (locale: 'zh' | 'en', zh: string, en: string) => locale === 'zh' ? zh : en;
const reasonText = (reason: unknown) => reason instanceof Error ? reason.message : String(reason);
const batchStateLabels: Record<BatchState, { zh: string; en: string }> = {
  queued: { zh: '等待执行', en: 'Queued' }, running: { zh: '执行中', en: 'Running' },
  succeeded: { zh: '全部成功', en: 'Succeeded' }, partially_succeeded: { zh: '部分成功', en: 'Partially succeeded' },
  failed: { zh: '全部失败', en: 'Failed' }, stopped: { zh: '已停止', en: 'Stopped' }, unknown: { zh: '结果未知', en: 'Unknown' },
};
const targetStateLabels: Record<TargetRunState, { zh: string; en: string }> = {
  queued: { zh: '未开始', en: 'Not started' }, running: { zh: '执行中', en: 'Running' },
  succeeded: { zh: '成功', en: 'Succeeded' }, failed: { zh: '失败', en: 'Failed' },
  stopped: { zh: '已停止', en: 'Stopped' }, unknown: { zh: '结果未知', en: 'Unknown' },
};

export function AutomationPanel({ targets, locale, onClose }: Props) {
  const [tab, setTab] = useState<Tab>('batch');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [command, setCommand] = useState('uptime');
  const [concurrency, setConcurrency] = useState(2);
  const [mode, setMode] = useState<ExecutionMode>('auto_except_high_risk');
  const [workflowAiStep, setWorkflowAiStep] = useState(false);
  const [busy, setBusy] = useState(false);
  const [stopRequested, setStopRequested] = useState(false);
  const [runId, setRunId] = useState<string | null>(null);
  const [batchResult, setBatchResult] = useState<BatchRun | null>(null);
  const [expandedOutput, setExpandedOutput] = useState<string | null>(null);
  const [workflowResult, setWorkflowResult] = useState<WorkflowExecution | null>(null);
  const [workflowTarget, setWorkflowTarget] = useState(targets[0]?.session.target.sessionId ?? '');
  const [expectedOutput, setExpectedOutput] = useState('');
  const [waitSeconds, setWaitSeconds] = useState(1);
  const [retries, setRetries] = useState(1);
  const [templateName, setTemplateName] = useState('');
  const [definitions, setDefinitions] = useState<DefinitionItem[]>([]);
  const [history, setHistory] = useState<AutomationRunRecord[]>([]);
  const [selectedHistory, setSelectedHistory] = useState<AutomationRunRecord | null>(null);
  const [error, setError] = useState('');
  const targetMap = useMemo(() => new Map(targets.map((item) => [item.session.target.sessionId, item])), [targets]);
  const pending = workflowResult?.pendingApproval ?? null;
  const batchProgress = useMemo(() => batchResult ? summarizeBatchProgress(batchResult) : null, [batchResult]);

  const loadHistory = async () => {
    try { setHistory(await desktop.automationRunHistory(50)); }
    catch (reason) { setError(reasonText(reason)); }
  };
  const loadDefinitions = async () => {
    try {
      const [batch, workflows] = await Promise.all([
        desktop.automationDefinitions('batch_template'), desktop.automationDefinitions('workflow_definition'),
      ]);
      setDefinitions([
        ...batch.map((item) => ({ ...item, kind: 'batch_template' as const })),
        ...workflows.map((item) => ({ ...item, kind: 'workflow_definition' as const })),
      ]);
    } catch (reason) { setError(reasonText(reason)); }
  };
  useEffect(() => { void loadDefinitions(); void loadHistory(); }, []);

  const chosenTargets = () => targets.filter((item) => selected.has(item.session.target.sessionId));
  const buildWorkflow = (target: TargetRef): WorkflowVersion => ({
    definitionId: crypto.randomUUID(), version: 1, startStep: 'check', maxTransitions: 20,
    createdAt: new Date().toISOString(), contentHash: '',
    steps: [
      { id: 'check', kind: { type: 'command', config: { target, command, source: workflowAiStep ? 'ai' : 'workflow', timeoutSeconds: 60, retries, retryDelaySeconds: 1 } }, onSuccess: 'wait', onFailure: 'failed' },
      { id: 'wait', kind: { type: 'wait', config: { seconds: waitSeconds } }, onSuccess: 'condition', onFailure: null },
      { id: 'condition', kind: { type: 'condition', config: { field: 'stdout_contains', value: expectedOutput } }, onSuccess: 'done', onFailure: 'failed' },
      { id: 'done', kind: { type: 'end' }, onSuccess: null, onFailure: null },
      { id: 'failed', kind: { type: 'failure' }, onSuccess: null, onFailure: null },
    ],
  });

  const runBatch = async () => {
    setError(''); setBatchResult(null); setStopRequested(false); setExpandedOutput(null);
    const chosen = chosenTargets();
    if (!chosen.length || !command.trim()) { setError(text(locale, '请选择目标并输入命令', 'Select targets and enter a command')); return; }
    setBusy(true);
    let finished = false;
    let polling = false;
    let timer: number | undefined;
    try {
      const id = await desktop.batchCreate(buildBatchSpec(command, chosen.map((item) => item.session.target), concurrency));
      setRunId(id);
      const refresh = async () => {
        if (finished || polling) return;
        polling = true;
        try {
          const snapshot = await desktop.batchSnapshot(id);
          if (!finished) setBatchResult(snapshot);
        } catch {
          // The awaited batch run reports fatal errors; a transient poll must not interrupt it.
        } finally { polling = false; }
      };
      const execution = desktop.batchRun(id);
      timer = window.setInterval(() => { void refresh(); }, 500);
      await refresh();
      const result = await execution;
      finished = true;
      if (timer !== undefined) window.clearInterval(timer);
      setBatchResult(result);
      await loadHistory();
    } catch (reason) { setError(reasonText(reason)); }
    finally {
      finished = true;
      if (timer !== undefined) window.clearInterval(timer);
      setBusy(false); setRunId(null); setStopRequested(false);
    }
  };

  const runWorkflow = async () => {
    setError(''); setWorkflowResult(null);
    const target = targetMap.get(workflowTarget)?.session.target;
    if (!target || !command.trim()) { setError(text(locale, '请选择已连接目标并输入命令', 'Select a connected target and enter a command')); return; }
    if (!expectedOutput) { setError(text(locale, '条件匹配内容不能为空', 'Expected output cannot be empty')); return; }
    setBusy(true);
    try {
      const id = await desktop.workflowCreate(buildWorkflow(target));
      setRunId(id);
      const result = await desktop.workflowRun(id, mode);
      setWorkflowResult(result);
      setRunId(result.run.state === 'awaiting_approval' ? id : null);
      await loadHistory();
    } catch (reason) { setError(reasonText(reason)); setRunId(null); }
    finally { setBusy(false); }
  };

  const approveWorkflow = async () => {
    if (!runId || !pending) return;
    setBusy(true); setError('');
    try {
      const result = await desktop.workflowApproveAndRun(runId, pending.approvalId, pending.command, mode);
      setWorkflowResult(result);
      setRunId(result.run.state === 'awaiting_approval' ? runId : null);
      await loadHistory();
    } catch (reason) { setError(reasonText(reason)); }
    finally { setBusy(false); }
  };

  const stop = async () => {
    if (!runId) return;
    const stoppingBatch = tab === 'batch';
    setError('');
    if (stoppingBatch) setStopRequested(true);
    try {
      if (stoppingBatch) {
        await desktop.batchStop(runId);
        setBatchResult(await desktop.batchSnapshot(runId));
      } else {
        await desktop.workflowStop(runId);
        if (!busy) {
          setWorkflowResult((current) => current ? { run: { ...current.run, state: 'stopped', finishedAt: new Date().toISOString() }, pendingApproval: null } : null);
          setRunId(null);
          await loadHistory();
        }
      }
    } catch (reason) {
      if (stoppingBatch) setStopRequested(false);
      setError(reasonText(reason));
    }
  };

  const saveTemplate = async () => {
    setError('');
    if (!templateName.trim()) { setError(text(locale, '请输入模板名称', 'Enter a template name')); return; }
    try {
      if (tab === 'batch') {
        const chosen = chosenTargets();
        if (!chosen.length) { setError(text(locale, '请先选择模板目标', 'Select template targets first')); return; }
        await desktop.automationDefinitionSave('batch_template', null, templateName, {
          command, serverIds: chosen.map((item) => item.server.id), maxConcurrent: concurrency,
        });
      } else if (tab === 'workflow') {
        const target = targetMap.get(workflowTarget)?.session.target;
        if (!target || !expectedOutput) { setError(text(locale, '请先完善流程目标和条件', 'Complete the workflow target and condition first')); return; }
        const version = buildWorkflow(target);
        await desktop.automationDefinitionSave('workflow_definition', null, templateName, {
          definitionId: version.definitionId, version: version.version, startStep: version.startStep,
          maxTransitions: version.maxTransitions,
          steps: version.steps.map((step) => step.kind.type === 'command' ? {
            ...step,
            kind: { type: 'command', config: {
              serverId: step.kind.config.target.serverId, command: step.kind.config.command,
              source: step.kind.config.source, timeoutSeconds: step.kind.config.timeoutSeconds,
              retries: step.kind.config.retries, retryDelaySeconds: step.kind.config.retryDelaySeconds,
            } },
          } : step),
        });
      } else return;
      setTemplateName('');
      await loadDefinitions();
      setTab('templates');
    } catch (reason) { setError(reasonText(reason)); }
  };

  const loadTemplate = (item: DefinitionItem) => {
    setError(''); setBatchResult(null); setWorkflowResult(null); setRunId(null);
    if (item.kind === 'batch_template') {
      const definition = item.definition as unknown as { command?: string; serverIds?: string[]; maxConcurrent?: number };
      if (typeof definition.command !== 'string' || !Array.isArray(definition.serverIds)) { setError(text(locale, '批量模板格式无效', 'Invalid batch template')); return; }
      const ids = new Set(definition.serverIds);
      const next = targets.filter((target) => ids.has(target.server.id));
      if (!next.length) { setError(text(locale, '模板目标当前均未连接', 'None of the template targets are connected')); return; }
      setCommand(definition.command);
      setConcurrency(Math.max(1, Math.min(16, Number(definition.maxConcurrent) || 1)));
      setSelected(new Set(next.map((target) => target.session.target.sessionId)));
      setTab('batch');
      return;
    }
    const definition = item.definition as unknown as { steps?: Array<{ kind: { type: string; config?: { serverId?: string; command?: string; source?: 'workflow' | 'ai'; retries?: number; retryDelaySeconds?: number; seconds?: number; value?: string } } }> };
    const steps = Array.isArray(definition.steps) ? definition.steps : [];
    const commandStep = steps.find((step) => step.kind.type === 'command')?.kind.config;
    const target = targets.find((item) => item.server.id === commandStep?.serverId);
    if (!target || typeof commandStep?.command !== 'string') { setError(text(locale, '流程目标当前未连接或模板无效', 'The workflow target is disconnected or the template is invalid')); return; }
    const condition = steps.find((step) => step.kind.type === 'condition')?.kind.config;
    const wait = steps.find((step) => step.kind.type === 'wait')?.kind.config;
    setWorkflowTarget(target.session.target.sessionId);
    setCommand(commandStep.command);
    setWorkflowAiStep(commandStep.source === 'ai');
    setRetries(Math.max(0, Math.min(5, Number(commandStep.retries) || 0)));
    setWaitSeconds(Math.max(1, Math.min(300, Number(wait?.seconds) || 1)));
    setExpectedOutput(typeof condition?.value === 'string' ? condition.value : '');
    setTab('workflow');
  };

  const deleteTemplate = async (item: DefinitionItem) => {
    if (!window.confirm(text(locale, `确认删除模板“${item.name}”？`, `Delete template “${item.name}”?`))) return;
    try { await desktop.automationDefinitionDelete(item.kind, item.id); await loadDefinitions(); }
    catch (reason) { setError(reasonText(reason)); }
  };

  const renderResult = () => <>
    {batchResult && <div className="automation-results">
      <strong>{text(locale, batchResult.state === 'queued' || batchResult.state === 'running' ? '批量执行' : '批量结果', batchResult.state === 'queued' || batchResult.state === 'running' ? 'Batch execution' : 'Batch result')} · {stopRequested ? text(locale, '正在停止', 'Stopping') : batchStateLabels[batchResult.state][locale]}</strong>
      {batchProgress && <section className="automation-progress" aria-live="polite">
        <div className="automation-progress-total">{text(locale, `总共 ${batchProgress.total} 台，已完成 ${batchProgress.completed} 台`, `${batchProgress.completed} of ${batchProgress.total} servers completed`)}</div>
        <div className="automation-progress-counts">
          <span className="running-text">{text(locale, '执行中', 'Running')} <b>{batchProgress.running}</b></span>
          <span className="success-text">{text(locale, '成功', 'Succeeded')} <b>{batchProgress.succeeded}</b></span>
          <span className={batchProgress.failed ? 'error' : ''}>{text(locale, '失败', 'Failed')} <b>{batchProgress.failed}</b></span>
          <span>{text(locale, '未开始', 'Not started')} <b>{batchProgress.queued}</b></span>
          {batchProgress.stopped > 0 && <span>{text(locale, '已停止', 'Stopped')} <b>{batchProgress.stopped}</b></span>}
          {batchProgress.unknown > 0 && <span className="error">{text(locale, '结果未知', 'Unknown')} <b>{batchProgress.unknown}</b></span>}
        </div>
      </section>}
      {batchResult.targets.map((result) => {
        const item = targetMap.get(result.target.sessionId);
        const stateClass = result.state === 'succeeded' ? 'success-text' : result.state === 'running' ? 'running-text' : result.state === 'failed' || result.state === 'unknown' ? 'error' : '';
        const resultDetail = result.state === 'queued' ? text(locale, '等待调度', 'Waiting to start') : result.state === 'running' ? text(locale, '等待服务器返回', 'Waiting for server result') : result.exitStatus == null ? text(locale, '退出码未知', 'Unknown exit') : `exit ${result.exitStatus}`;
        const isExpanded = expandedOutput === result.target.sessionId;
        const outputId = `batch-output-${result.target.sessionId}`;
        const canViewOutput = result.executionId !== null && result.state !== 'queued' && result.state !== 'running';
        return <div className="automation-result-row" key={result.target.sessionId}>
          <span>{item?.server.name ?? result.target.serverId}</span>
          <b className={stateClass}>{targetStateLabels[result.state][locale]}</b>
          <small>{resultDetail}{result.errorCode ? ` · ${result.errorCode}` : ''}</small>
          {canViewOutput ? <button type="button" className="automation-output-toggle" aria-expanded={isExpanded} aria-controls={outputId} onClick={() => setExpandedOutput((current) => current === result.target.sessionId ? null : result.target.sessionId)}>
            {isExpanded ? <ChevronUp size={12} /> : <ChevronDown size={12} />}{text(locale, isExpanded ? '收起输出' : '查看输出', isExpanded ? 'Hide output' : 'View output')}
          </button> : <span />}
          {isExpanded && <section className="automation-output" id={outputId}>
            <div><strong>{text(locale, '标准输出', 'Standard output')}</strong><pre>{result.stdout || text(locale, '（无标准输出）', '(No standard output)')}</pre></div>
            {result.stderr && <div><strong>{text(locale, '错误输出', 'Standard error')}</strong><pre className="error">{result.stderr}</pre></div>}
          </section>}
        </div>;
      })}
    </div>}
    {workflowResult && <div className="automation-results"><strong>{text(locale, '流程结果', 'Workflow result')} · {workflowResult.run.state} · {workflowResult.run.versionHash.slice(0, 12)}</strong>{workflowResult.run.steps.map((step, index) => <div key={`${step.stepId}-${index}`}><span>{step.stepId}</span><b className={step.state === 'succeeded' ? 'success-text' : 'error'}>{step.state}</b><small>{step.exitStatus == null ? text(locale, '退出码未知', 'Unknown exit') : `exit ${step.exitStatus}`}</small>{step.stdout && <pre>{step.stdout}</pre>}{step.stderr && <pre className="error">{step.stderr}</pre>}</div>)}</div>}
  </>;

  return <div className="modal-backdrop"><section className="modal automation-modal" role="dialog" aria-modal="true" aria-label={text(locale, '批量与自动化', 'Batch and automation')}>
    <header><h2>{text(locale, '批量与自动化', 'Batch and automation')}</h2><button className="icon" onClick={onClose} disabled={busy} aria-label="Close"><X size={17} /></button></header>
    <div className="automation-tabs">
      <button className={tab === 'batch' ? 'active' : ''} disabled={busy} onClick={() => setTab('batch')}>{text(locale, '批量命令', 'Batch')}</button>
      <button className={tab === 'workflow' ? 'active' : ''} disabled={busy} onClick={() => setTab('workflow')}><Workflow size={14} />{text(locale, '手动流程', 'Workflow')}</button>
      <button className={tab === 'templates' ? 'active' : ''} disabled={busy} onClick={() => setTab('templates')}><Save size={14} />{text(locale, '模板', 'Templates')}</button>
      <button className={tab === 'history' ? 'active' : ''} disabled={busy} onClick={() => { setTab('history'); void loadHistory(); }}><History size={14} />{text(locale, '本地历史', 'History')}</button>
    </div>
    <div className="automation-body">
      <div className="inline-notice"><AlertTriangle size={15} /><span>{text(locale, '命令只在本机经 ExecutionBroker 执行。批量不是跨服务器事务；历史永不同步，模板只包含定义和服务器 ID。', 'Commands run locally through ExecutionBroker. Batch is not transactional; history never syncs and templates contain only definitions and server IDs.')}</span></div>
      {(tab === 'batch' || tab === 'workflow') && <>
        {tab === 'batch' ? <div className="automation-targets"><strong>{text(locale, '执行目标（已连接会话）', 'Targets (connected sessions)')}</strong>{targets.map((item) => <label key={item.session.target.sessionId}><input type="checkbox" disabled={busy} checked={selected.has(item.session.target.sessionId)} onChange={() => setSelected((current) => { const next = new Set(current); next.has(item.session.target.sessionId) ? next.delete(item.session.target.sessionId) : next.add(item.session.target.sessionId); return next; })} /><span>{item.server.name}<small>{item.server.username}@{item.server.host} · generation {item.session.target.generation}</small></span></label>)}</div> : <label>{text(locale, '流程目标', 'Workflow target')}<select disabled={busy || !!pending} value={workflowTarget} onChange={(event) => setWorkflowTarget(event.target.value)}>{targets.map((item) => <option key={item.session.target.sessionId} value={item.session.target.sessionId}>{item.server.name} · generation {item.session.target.generation}</option>)}</select></label>}
        <label>{text(locale, 'Linux 命令', 'Linux command')}<textarea className="automation-command" disabled={busy || !!pending} rows={3} maxLength={16384} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={command} onChange={(event) => setCommand(event.target.value)} /></label>
        <div className="automation-settings">
          {tab === 'batch' ? <>
            <label>{text(locale, '并发上限', 'Concurrency')}<input type="number" min={1} max={16} disabled={busy} value={concurrency} onChange={(event) => setConcurrency(Math.max(1, Math.min(16, Number(event.target.value))))} /></label>
            <label>{text(locale, '批量执行模式', 'Batch execution mode')}<select disabled value="auto_all" aria-label={text(locale, '批量执行模式', 'Batch execution mode')}><option value="auto_all">{text(locale, '自动执行所有命令', 'Auto all')}</option></select></label>
          </> : <>
            <label>{text(locale, '失败重试次数', 'Retries')}<input type="number" min={0} max={5} disabled={busy || !!pending} value={retries} onChange={(event) => setRetries(Math.max(0, Math.min(5, Number(event.target.value))))} /></label>
            <label>{text(locale, '等待秒数', 'Wait seconds')}<input type="number" min={1} max={300} disabled={busy || !!pending} value={waitSeconds} onChange={(event) => setWaitSeconds(Math.max(1, Math.min(300, Number(event.target.value))))} /></label>
            <label>{text(locale, 'AI 步骤执行模式', 'AI step execution mode')}<select disabled={busy || !!pending || !workflowAiStep} value={mode} onChange={(event) => setMode(event.target.value as ExecutionMode)}><option value="confirm_each">{text(locale, '确认执行', 'Confirm each')}</option><option value="auto_except_high_risk">{text(locale, '自动执行（高危确认）', 'Auto except high-risk')}</option><option value="auto_all">{text(locale, '自动执行所有命令', 'Auto all')}</option></select></label>
          </>}
        </div>
        {tab === 'workflow' && <>
          <label>{text(locale, '成功分支：标准输出包含', 'Success branch: stdout contains')}<input disabled={busy || !!pending} maxLength={256} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={expectedOutput} onChange={(event) => setExpectedOutput(event.target.value)} placeholder={text(locale, '例如 ready；不匹配则进入失败分支', 'For example ready; otherwise take failure branch')} /></label>
          <label className="automation-check"><input type="checkbox" disabled={busy || !!pending} checked={workflowAiStep} onChange={(event) => setWorkflowAiStep(event.target.checked)} />{text(locale, '将命令步骤作为 AI 步骤（应用上述审批模式）', 'Run command as an AI step (apply the approval mode above)')}</label>
        </>}
        <div className="automation-save"><input maxLength={128} disabled={busy || !!pending} autoCapitalize="none" autoCorrect="off" spellCheck={false} value={templateName} onChange={(event) => setTemplateName(event.target.value)} placeholder={text(locale, '模板名称', 'Template name')} /><button className="button secondary" disabled={busy || !!pending} onClick={() => void saveTemplate()}><Save size={14} />{text(locale, '保存为模板', 'Save template')}</button></div>
      </>}
      {tab === 'templates' && <div className="automation-list">{definitions.length === 0 && <p className="muted">{text(locale, '暂无模板，请在批量或流程页保存。', 'No templates yet. Save one from Batch or Workflow.')}</p>}{definitions.map((item) => <div className="automation-list-row" key={`${item.kind}-${item.id}`}><span><strong>{item.name}</strong><small>{item.kind} · revision {item.revision} · {new Date(item.updatedAt).toLocaleString()}</small></span><button className="button secondary" onClick={() => loadTemplate(item)}>{text(locale, '载入', 'Load')}</button><button className="icon danger" aria-label={text(locale, '删除模板', 'Delete template')} onClick={() => void deleteTemplate(item)}><Trash2 size={15} /></button></div>)}</div>}
      {tab === 'history' && <div className="automation-list">{history.length === 0 && <p className="muted">{text(locale, '暂无本地运行历史。', 'No local run history.')}</p>}{history.map((item) => <button className={`automation-list-row history-choice ${selectedHistory?.id === item.id ? 'active' : ''}`} key={item.id} onClick={() => setSelectedHistory(item)}><span><strong>{item.kind} · {item.state}</strong><small>{item.id} · {new Date(item.createdAt).toLocaleString()}</small></span></button>)}{selectedHistory && <pre className="automation-history-detail">{JSON.stringify(selectedHistory.payload, null, 2)}</pre>}</div>}
      {pending && <div className="automation-approval"><strong>{text(locale, 'AI 步骤等待确认', 'AI step approval required')}</strong><code>{pending.command}</code><small>{pending.risk.level} · {pending.risk.reasons.join(' · ')} · {new Date(pending.expiresAt).toLocaleString()}</small><button className="button primary" disabled={busy} onClick={() => void approveWorkflow()}><Play size={14} />{text(locale, '确认并从当前步骤继续', 'Approve and resume current step')}</button></div>}
      {error && <p className="error" role="alert">{error}</p>}
      {renderResult()}
    </div>
    <footer className="automation-footer"><span>{runId ? `run ${runId}` : text(locale, '重启后的未完成运行会标记为结果未知', 'Interrupted runs are marked unknown after restart')}</span>{(busy || pending) && runId ? <button className="button danger" disabled={tab === 'batch' && stopRequested} onClick={() => void stop()}><Square size={14} />{tab === 'batch' && stopRequested ? text(locale, '停止中…', 'Stopping…') : text(locale, '停止', 'Stop')}</button> : (tab === 'batch' || tab === 'workflow') ? <button className="button primary" disabled={busy || !!pending} onClick={() => void (tab === 'batch' ? runBatch() : runWorkflow())}><Play size={14} />{text(locale, '开始执行', 'Run')}</button> : <span />}</footer>
    {busy && <RefreshCw className="automation-spin spin" size={15} />}
  </section></div>;
}

