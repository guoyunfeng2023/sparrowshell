import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { desktop } from '../../ipc/client';
import { messages } from '../../i18n';
import type { AiAgentTurnResult, ExecutionMode, SessionSnapshot, StepProgress, TargetRef } from '../../types';
import { AiPanel } from './AiPanel';
import type { AiEntitlement } from '../../types';

const firstTarget: TargetRef = {
  serverId: '11111111-1111-4111-8111-111111111111',
  sessionId: '22222222-2222-4222-8222-222222222222',
  generation: 1,
};
const secondTarget: TargetRef = { ...firstTarget, sessionId: '33333333-3333-4333-8333-333333333333' };
const sessionFor = (target: TargetRef): SessionSnapshot => ({ target, state: 'connected', channelId: null, errorCode: null });
const t = (key: keyof typeof messages.zh) => messages.zh[key];
const skipLiveAnalysis = import.meta.env.VITE_SPARROWSHELL_E2E_SKIP_LIVE_ANALYSIS === '1';

// Each native acceptance run must prove the revoked-permission path, independent of prior runs.
localStorage.setItem('sparrowshell.ai.context.v2', JSON.stringify({
  serverLabel: false, systemInfo: false, workingDirectory: false,
  selectedOutput: false, recentResults: false, directoryEntries: false,
}));

function waitFor<T>(read: () => T | null | false, timeoutMs = 15000): Promise<T> {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const poll = () => {
      try {
        const value = read();
        if (value) return resolve(value);
        if (Date.now() - started > timeoutMs) {
          const currentView = document.querySelector('.ai-panel')?.textContent?.replace(/\s+/g, ' ').slice(0, 1200) ?? '无 AI 面板';
          return reject(new Error(`等待界面状态超时；当前界面：${currentView}`));
        }
        window.setTimeout(poll, 50);
      } catch (reason) {
        reject(reason);
      }
    };
    poll();
  });
}

function clickButton(text: string) {
  const button = Array.from(document.querySelectorAll('button')).find((item) => item.textContent?.includes(text));
  if (!button) throw new Error(`找不到按钮：${text}`);
  button.click();
}

async function submitPrompt(text: string) {
  const textarea = await waitFor(() => document.querySelector<HTMLTextAreaElement>('.ai-input-wrap textarea'));
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
  if (!setter) throw new Error('无法设置验收输入');
  setter.call(textarea, text);
  textarea.dispatchEvent(new Event('input', { bubbles: true }));
  const send = await waitFor(() => {
    const button = document.querySelector<HTMLButtonElement>('.send-button');
    return button && !button.disabled ? button : null;
  });
  send.click();
}

async function selectMode(label: string) {
  const mode = await waitFor(() => document.querySelector<HTMLButtonElement>('.mode-button'));
  mode.click();
  const option = await waitFor(() => Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')).find((item) => item.textContent?.includes(label)) ?? null);
  option.click();
  await waitFor(() => document.querySelector('.mode-button')?.textContent?.includes(label) ? true : null);
}

function commandCard(command: string) {
  return Array.from(document.querySelectorAll<HTMLElement>('.command-card')).find((card) => card.querySelector('code')?.textContent === command) ?? null;
}

async function syntheticExecutionCount() {
  return invoke<number>('ai_e2e_execution_count');
}

async function runBillingLicenseAcceptance() {
  const originalEntitlement = desktop.aiEntitlement;
  const originalLicenseKey = desktop.aiLicenseKey;
  const license = 'SS-1111-2222-3333-4444-5555-6666-7777-8888';
  const entitlement: AiEntitlement = {
    plan: 'personal', licensed: true, bound: true, quota: 200, used: 3, remaining: 197,
    periodStart: '2026-11-10T00:00:00Z', periodEnd: '2026-12-10T00:00:00Z',
    expired: false, previousPlan: null, previousPeriodEnd: null, paymentEnabled: true, mockPayment: true, paymentAmountOverride: null,
  };
  let storedLicense: string | null = license;
  let failure = '';
  desktop.aiEntitlement = async () => entitlement;
  desktop.aiLicenseKey = async () => {
    if (failure) throw new Error(failure);
    return storedLicense;
  };
  try {
    clickButton('充值');
    await waitFor(() => document.querySelector('.billing-saved-license'));
    clickButton('显示激活码');
    await waitFor(() => document.querySelector('.billing-saved-license code')?.textContent === license ? true : null);
    clickButton('隐藏');
    await waitFor(() => document.querySelector('.billing-saved-license code')?.textContent !== license ? true : null);

    storedLicense = null;
    clickButton('显示激活码');
    const missingError = await waitFor(() => document.querySelector<HTMLElement>('.billing-license-error'));
    if (!missingError.textContent?.includes('本机未保存激活码')) throw new Error('激活码为空时没有在操作区域提示');

    storedLicense = license;
    failure = '合成读取失败';
    clickButton('显示激活码');
    const readError = await waitFor(() => {
      const element = document.querySelector<HTMLElement>('.billing-license-error');
      return element?.textContent?.includes(failure) ? element : null;
    });
    if (!readError.textContent?.includes(failure)) throw new Error('读取失败时没有在操作区域显示具体原因');

    failure = '';
    clickButton('显示激活码');
    await waitFor(() => document.querySelector('.billing-saved-license code')?.textContent === license && !document.querySelector('.billing-license-error') ? true : null);
    document.querySelector<HTMLButtonElement>('.ai-billing-modal>header button')?.click();
    await waitFor(() => !document.querySelector('.ai-billing-modal') ? true : null);
  } finally {
    desktop.aiEntitlement = originalEntitlement;
    desktop.aiLicenseKey = originalLicenseKey;
  }
}

async function runNativeUiAcceptance(switchTarget: (target: TargetRef) => void) {
  const originalAgentTurn = desktop.aiAgentTurn;
  const originalRunNext = desktop.aiRunNext;
  const originalApprove = desktop.aiApproveAndRun;
  const originalAnalyze = desktop.aiAnalyze;
  const thirdTarget: TargetRef = { ...firstTarget, sessionId: '44444444-4444-4444-8444-444444444444' };
  const fourthTarget: TargetRef = { ...firstTarget, sessionId: '55555555-5555-4555-8555-555555555555' };
  const normalCommand = 'printf sparrowshell-native-approval';
  const highRiskCommand = 'rm -rf /tmp/sparrowshell-native-ui-only';
  let analyzeCalls = 0;
  let approvalCalls = 0;
  desktop.aiAgentTurn = async (_intent, _taskId, _history, _locale, target, _input, _permissions, executionIds, sessionId): Promise<AiAgentTurnResult> => {
    const responseSessionId = sessionId ?? `qwen-session-${target.sessionId}`;
    if (executionIds.length > 0) {
      return { type: 'final_answer', answer: '本地 UI 验收：命令已执行，动态任务完成', sessionId: responseSessionId };
    }
    const command = target.sessionId === secondTarget.sessionId
      ? normalCommand
      : [thirdTarget.sessionId, fourthTarget.sessionId].includes(target.sessionId)
        ? highRiskCommand
        : 'test -f /tmp/sparrowshell-native-ui-only';
    return {
      type: 'tool_call',
      summary: '本地 UI 验收：生成一条合成命令提案',
      plan: await desktop.aiCreatePlan(target, [{ command, modelRisk: null }]),
      sessionId: responseSessionId,
    };
  };
  desktop.aiRunNext = (planId: string, target: TargetRef, mode: ExecutionMode) => invoke<StepProgress>('ai_e2e_run_next', { planId, target, mode });
  desktop.aiApproveAndRun = (planId: string, approvalId: string, command: string, target: TargetRef) => {
    approvalCalls += 1;
    return invoke<StepProgress>('ai_e2e_approve_and_run', { planId, approvalId, command, target });
  };
  desktop.aiAnalyze = (...args) => { analyzeCalls += 1; return originalAnalyze(...args); };

  try {
    await runBillingLicenseAcceptance();
    await submitPrompt('生成原生 UI 验收记录');
    const analysisButton = await waitFor(() => document.querySelector<HTMLButtonElement>('.analysis-button'));
    const originalOutput = Array.from(document.querySelectorAll('.command-result pre')).map((item) => item.textContent);
    if (originalOutput.join('|') !== 'synthetic stdout: checked|synthetic stderr: missing file') {
      throw new Error(`合成原始输出不符合预期：${originalOutput.join('|')}`);
    }

    let analysisText: string | null = null;
    if (!skipLiveAnalysis) {
      analysisButton.click();
      await waitFor(() => document.querySelector('.command-analysis.error'));
      if (analyzeCalls !== 0) throw new Error('未授权时仍然调用了分析接口');

      if (!document.querySelector('.context-permissions')) clickButton(t('contextPermissions'));
      const permission = await waitFor(() => {
        const label = Array.from(document.querySelectorAll('.context-permissions label')).find((item) => item.textContent?.includes(t('recentResults')));
        return label?.querySelector<HTMLInputElement>('input') ?? null;
      });
      permission.click();
      await waitFor(() => permission.checked ? true : null);
      clickButton(t('analyzeResult'));
      const analysis = await waitFor(() => document.querySelector<HTMLElement>('.command-analysis.success'), 90000);
      if (Number(analyzeCalls) !== 1) throw new Error(`授权后的分析请求次数异常：${analyzeCalls}`);
      if (!(analysis.textContent ?? '').includes(t('analysisReadOnly'))) throw new Error('缺少只读分析说明');
      const unchangedOutput = Array.from(document.querySelectorAll('.command-result pre')).map((item) => item.textContent);
      if (unchangedOutput.join('|') !== originalOutput.join('|')) throw new Error('分析覆盖了原始执行输出');
      analysisText = analysis.textContent;
    }

    if (skipLiveAnalysis) {
      const permission = await waitFor(() => {
        const label = Array.from(document.querySelectorAll('.context-permissions label')).find((item) => item.textContent?.includes(t('recentResults')));
        return label?.querySelector<HTMLInputElement>('input') ?? null;
      });
      permission.click();
    }
    const continueButton = await waitFor(() => Array.from(document.querySelectorAll<HTMLButtonElement>('button'))
      .find((item) => item.textContent?.includes(t('continueAgent')) && !item.disabled) ?? null);
    continueButton.click();
    await waitFor(() => Array.from(document.querySelectorAll('.analysis-answer'))
      .filter((item) => item.textContent?.includes('本地 UI 验收：命令已执行，动态任务完成')).length === 1 ? true : null);

    await submitPrompt('验证多轮对话时间线');
    await waitFor(() => Array.from(document.querySelectorAll('.analysis-answer'))
      .filter((item) => item.textContent?.includes('本地 UI 验收：命令已执行，动态任务完成')).length === 2 ? true : null);
    const messagesInOrder = Array.from(document.querySelectorAll('.ai-message'));
    const firstPromptIndex = messagesInOrder.findIndex((item) => item.textContent?.includes('生成原生 UI 验收记录'));
    const firstAnswerIndex = messagesInOrder.findIndex((item) => item.textContent?.includes('本地 UI 验收：命令已执行，动态任务完成'));
    const secondPromptIndex = messagesInOrder.findIndex((item) => item.textContent?.includes('验证多轮对话时间线'));
    const secondAnswerIndex = messagesInOrder.findIndex((item, index) => index > secondPromptIndex && item.textContent?.includes('本地 UI 验收：命令已执行，动态任务完成'));
    if (!(firstPromptIndex >= 0 && firstPromptIndex < firstAnswerIndex && firstAnswerIndex < secondPromptIndex && secondPromptIndex < secondAnswerIndex)) {
      throw new Error('多轮对话没有按照问题、回答、问题、回答的顺序展示');
    }
    const timeline = Array.from(document.querySelectorAll('.ai-message, .command-card'));
    const secondSummaryIndex = timeline.findIndex((item, index) => index > 0 && item.classList.contains('ai-message') && item.textContent?.includes('验证多轮对话时间线'));
    const secondCommandIndex = timeline.findIndex((item, index) => index > secondSummaryIndex && item.classList.contains('command-card'));
    const secondFinalIndex = timeline.findIndex((item, index) => index > secondCommandIndex && item.classList.contains('analysis-answer'));
    if (!(secondSummaryIndex >= 0 && secondSummaryIndex < secondCommandIndex && secondCommandIndex < secondFinalIndex)) {
      throw new Error('命令卡片没有展示在对应摘要和最终回答之间');
    }

    switchTarget(secondTarget);
    await waitFor(() => !document.querySelector('.command-result') && !document.querySelector('.command-analysis') ? true : null);
    if (document.querySelector('.command-analysis')) throw new Error('切换会话后仍显示旧分析');

    switchTarget(firstTarget);
    const restoredCard = await waitFor(() => {
      const card = commandCard('test -f /tmp/sparrowshell-native-ui-only');
      return card?.querySelector('.command-result') ? card : null;
    });
    if (restoredCard.querySelector('.command-review') || document.querySelector('.ai-actions button')) {
      throw new Error('切回会话后恢复了可执行的旧任务状态');
    }
    if (!document.querySelector('.ai-conversation')?.textContent?.includes('验证多轮对话时间线')) {
      throw new Error('切回会话后没有恢复历史对话');
    }
    switchTarget(secondTarget);
    await waitFor(() => !document.querySelector('.ai-plan') ? true : null);

    await selectMode(t('confirmEach'));
    const normalBefore = await syntheticExecutionCount();
    await submitPrompt('逐条确认普通提案');
    const normalPending = await waitFor(() => {
      const card = commandCard(normalCommand);
      return card?.querySelector('.command-review') ? card : null;
    });
    if (normalPending.classList.contains('high-risk')) throw new Error('普通提案被错误标记为高危');
    if (!normalPending.textContent?.includes(t('waitingApproval'))) throw new Error('普通提案未进入等待确认状态');
    if (normalPending.querySelector('.command-result')) throw new Error('普通提案在确认前已经出现执行结果');
    if (Number(approvalCalls) !== 0) throw new Error('普通提案在确认前调用了审批执行');
    if (await syntheticExecutionCount() !== normalBefore) throw new Error('普通提案在确认前实际执行了命令');

    normalPending.querySelector<HTMLButtonElement>('.command-review button')?.click();
    const normalCompleted = await waitFor(() => {
      const card = commandCard(normalCommand);
      return card?.querySelector('.command-result') && !card.querySelector('.command-review') ? card : null;
    });
    if (!normalCompleted.textContent?.includes('synthetic stdout: checked')) throw new Error('普通提案确认后未展示合成执行结果');
    if (Number(approvalCalls) !== 1) throw new Error(`普通提案确认后的审批调用次数异常：${approvalCalls}`);
    const normalAfter = await syntheticExecutionCount();
    if (normalAfter !== normalBefore + 1) throw new Error('普通提案确认后不是恰好执行一次');

    switchTarget(thirdTarget);
    await waitFor(() => !document.querySelector('.ai-plan') ? true : null);
    await selectMode(t('autoExcept'));
    const highRiskBefore = await syntheticExecutionCount();
    await submitPrompt('生成需要确认的高危提案');
    const highRiskPending = await waitFor(() => {
      const card = commandCard(highRiskCommand);
      return card?.classList.contains('high-risk') && card.querySelector('.command-review') ? card : null;
    });
    if (!highRiskPending.textContent?.includes(t('highRisk')) || !highRiskPending.textContent.includes(t('waitingApproval'))) {
      throw new Error('高危提案缺少高危标识或等待确认状态');
    }
    if (highRiskPending.querySelector('.command-result')) throw new Error('高危提案在确认前已经出现执行结果');
    if (Number(approvalCalls) !== 1) throw new Error('高危提案在确认前调用了审批执行');
    if (await syntheticExecutionCount() !== highRiskBefore) throw new Error('高危提案在确认前实际执行了命令');

    highRiskPending.querySelector<HTMLButtonElement>('.command-review button')?.click();
    const highRiskCompleted = await waitFor(() => {
      const card = commandCard(highRiskCommand);
      return card?.querySelector('.command-result') && !card.querySelector('.command-review') ? card : null;
    });
    if (!highRiskCompleted.classList.contains('high-risk')) throw new Error('高危提案执行后丢失高危标识');
    if (Number(approvalCalls) !== 2) throw new Error(`高危提案确认后的审批调用次数异常：${approvalCalls}`);
    const highRiskAfter = await syntheticExecutionCount();
    if (highRiskAfter !== highRiskBefore + 1) throw new Error('高危提案确认后不是恰好执行一次');

    switchTarget(fourthTarget);
    await waitFor(() => !document.querySelector('.ai-plan') ? true : null);
    await selectMode(t('autoAll'));
    const autoAllBefore = await syntheticExecutionCount();
    await submitPrompt('自动执行全部命令矩阵验收');
    const autoAllCompleted = await waitFor(() => {
      const card = commandCard(highRiskCommand);
      return card?.querySelector('.command-result') && !card.querySelector('.command-review') ? card : null;
    });
    if (!autoAllCompleted.classList.contains('high-risk')) throw new Error('AutoAll 执行后丢失高危标识');
    if (Number(approvalCalls) !== 2) throw new Error('AutoAll 高危命令触发了额外审批');
    const autoAllAfter = await syntheticExecutionCount();
    if (autoAllAfter !== autoAllBefore + 1) throw new Error('AutoAll 高危命令不是直接恰好执行一次');

    const report = {
      status: 'passed',
      analyzeCalls,
      approvalCalls,
      originalOutput,
      analysis: analysisText,
      liveAnalysisSkipped: skipLiveAnalysis,
      executionCounts: { normalBefore, normalAfter, highRiskBefore, highRiskAfter, autoAllBefore, autoAllAfter },
      checks: [
        'license-reveal-and-hide',
        'license-missing-inline-error',
        'license-read-failure-inline-error',
        'license-retry-after-failure',
        ...(skipLiveAnalysis ? ['live-analysis-skipped'] : ['unauthorized-request-blocked', 'real-analysis-rendered', 'original-output-preserved']),
        'target-change-cleared',
        'target-change-restored-conversation',
        'target-restore-not-executable',
        'multi-turn-timeline-interleaved',
        'command-card-follows-own-summary',
        'confirm-each-waits-for-approval',
        'normal-approval-runs-exactly-once',
        'auto-except-high-risk-requires-confirmation',
        'high-risk-approval-runs-exactly-once',
        'auto-all-high-risk-runs-without-additional-approval',
        'auto-all-high-risk-runs-exactly-once',
      ],
    };
    await invoke('ai_e2e_report', { report: JSON.stringify(report) });
    return report;
  } finally {
    desktop.aiAgentTurn = originalAgentTurn;
    desktop.aiRunNext = originalRunNext;
    desktop.aiApproveAndRun = originalApprove;
    desktop.aiAnalyze = originalAnalyze;
  }
}

export function AiE2eApp() {
  const [session, setSession] = useState(() => sessionFor(firstTarget));
  const [status, setStatus] = useState('正在运行原生 UI 验收…');
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void runNativeUiAcceptance((target) => setSession(sessionFor(target)))
      .then(() => setStatus('原生 UI 验收通过'))
      .catch(async (reason: unknown) => {
        const message = reason instanceof Error ? reason.message : String(reason);
        setStatus(`原生 UI 验收失败：${message}`);
        await invoke('ai_e2e_report', { report: JSON.stringify({ status: 'failed', message }) }).catch(() => {});
      });
  }, []);

  return <main className="ai-e2e-shell">
    <header><strong>SparrowShell 原生 UI 验收</strong><span role="status">{status}</span></header>
    <AiPanel session={session} serverName="本地合成验收目标" connected locale="zh" maxSteps={20} t={t} />
  </main>;
}
