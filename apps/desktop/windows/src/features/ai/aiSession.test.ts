import { describe, expect, it } from 'vitest';
import { aiReducer, initialAiState } from './aiSession';
import type { AiPlan, StepProgress, TargetRef } from '../../types';

const target: TargetRef = { serverId: 'server-1', sessionId: 'session-1', generation: 1 };
const plan: AiPlan = {
  planId: 'plan-1', target, state: 'awaiting_approval', currentStep: 0,
  steps: [{ proposal: { command: 'rm -rf /tmp/demo', modelRisk: null }, state: 'awaiting_approval', executionId: 'execution-1', approvalId: 'approval-1' }],
};
const approval: StepProgress = {
  plan,
  approval: {
    approvalId: 'approval-1', executionId: 'execution-1', target,
    command: 'rm -rf /tmp/demo', commandHash: 'hash-1',
    risk: { level: 'high', reasons: ['recursive_delete'] }, expiresAt: '2026-10-29T12:00:00Z',
  },
  execution: null,
};

describe('AI workbench state', () => {
  it('defaults to high-risk confirmation and keeps the mode continuously visible', () => {
    expect(initialAiState.mode).toBe('auto_except_high_risk');
    const next = aiReducer(initialAiState, { type: 'mode_changed', mode: 'auto_all' });
    expect(next.mode).toBe('auto_all');
    expect(next.plan).toBeNull();
  });

  it('records the user prompt without fabricating an assistant proposal', () => {
    const next = aiReducer(initialAiState, { type: 'prompt_sent', id: 'message-1', text: '检查磁盘', target });
    expect(next.messages).toEqual([{ id: 'message-1', role: 'user', text: '检查磁盘' }]);
    expect(next.plan).toBeNull();
    expect(next.waitingForProvider).toBe(true);
  });

  it('renders only broker-issued approvals and real execution results', () => {
    const waiting = aiReducer(initialAiState, { type: 'progress_received', progress: approval });
    expect(waiting.pendingApproval?.approvalId).toBe('approval-1');
    expect(waiting.plan?.steps[0].proposal.command).toBe('rm -rf /tmp/demo');
    const completed: StepProgress = {
      plan: { ...plan, state: 'completed', currentStep: null, steps: [{ ...plan.steps[0], state: 'succeeded', approvalId: null }] },
      approval: null,
      execution: {
        executionId: 'execution-1', source: 'ai', target, mode: 'auto_except_high_risk',
        command: 'rm -rf /tmp/demo', commandHash: 'hash-1', risk: approval.approval!.risk,
        state: 'succeeded', approvalId: 'approval-1', approvalExpiresAt: null,
        stdout: 'actual remote output\n', stderr: '', exitStatus: 0, errorCode: null,
        createdAt: '2026-10-29T11:00:00Z', startedAt: '2026-10-29T11:00:01Z', finishedAt: '2026-10-29T11:00:02Z',
      },
    };
    const done = aiReducer(waiting, { type: 'progress_received', progress: completed });
    expect(done.pendingApproval).toBeNull();
    expect(done.executions['execution-1'].stdout).toBe('actual remote output\n');
    expect(done.waitingForProvider).toBe(false);
  });

  it('adds real analysis as an assistant answer without changing execution records', () => {
    const started = aiReducer(initialAiState, { type: 'target_changed', target });
    const next = aiReducer(started, {
      type: 'analysis_received', target,
      message: { id: 'answer-1', role: 'assistant', kind: 'analysis', text: '服务器尚未安装 Docker。' },
    });
    expect(next.messages).toEqual([{ id: 'answer-1', role: 'assistant', kind: 'analysis', text: '服务器尚未安装 Docker。' }]);
    expect(next.executions).toEqual(started.executions);
  });

  it('keeps every dynamic command plan and updates only the matching plan', () => {
    const started = aiReducer(initialAiState, { type: 'target_changed', target });
    const first = aiReducer(started, {
      type: 'provider_plan_received', message: { id: 'summary-1', role: 'assistant', kind: 'plan', text: '第一步' },
      plan: { ...plan, planId: 'plan-1', state: 'ready' },
    });
    const secondPlan = { ...plan, planId: 'plan-2', state: 'ready' as const };
    const second = aiReducer(first, {
      type: 'provider_plan_received', message: { id: 'summary-2', role: 'assistant', kind: 'plan', text: '第二步' }, plan: secondPlan,
    });
    expect(second.plans.map((item) => item.planId)).toEqual(['plan-1', 'plan-2']);
    expect(second.messages.map((item) => item.planId)).toEqual(['plan-1', 'plan-2']);
    const progressed = aiReducer(second, { type: 'progress_received', progress: { ...approval, plan: { ...secondPlan, state: 'awaiting_approval' } } });
    expect(progressed.plans[0].planId).toBe('plan-1');
    expect(progressed.plans[1].state).toBe('awaiting_approval');
  });

  it('starts a new task without deleting conversation history', () => {
    const started = aiReducer(initialAiState, { type: 'target_changed', target });
    const withAnswer = aiReducer(started, {
      type: 'analysis_received', target, message: { id: 'answer', role: 'assistant', kind: 'analysis', text: '上次回答' },
    });
    const next = aiReducer(withAnswer, { type: 'prompt_sent', id: 'next', text: '继续检查为什么', target });
    expect(next.messages.map((message) => message.text)).toEqual(['上次回答', '继续检查为什么']);
    expect(next.plan).toBeNull();
    expect(next.plans).toEqual([]);
    expect(next.executions).toEqual({});
  });

  it('invalidates old plans and approvals when the server session or generation changes', () => {
    const waiting = aiReducer(initialAiState, { type: 'progress_received', progress: approval });
    const reconnected = { ...target, generation: 2 };
    const next = aiReducer(waiting, { type: 'target_changed', target: reconnected });
    expect(next.target).toEqual(reconnected);
    expect(next.plan).toBeNull();
    expect(next.pendingApproval).toBeNull();
    expect(next.executions).toEqual({});
    expect(next.messages).toEqual([]);
  });

  it('restores conversation results without restoring executable task state', () => {
    const execution: StepProgress['execution'] = {
      executionId: 'execution-1', source: 'ai', target, mode: 'auto_except_high_risk',
      command: 'docker --version', commandHash: 'hash-1', risk: { level: 'low', reasons: [] },
      state: 'succeeded', approvalId: null, approvalExpiresAt: null,
      stdout: 'Docker version 27.0.0\n', stderr: '', exitStatus: 0, errorCode: null,
      createdAt: '2026-10-29T11:00:00Z', startedAt: '2026-10-29T11:00:01Z', finishedAt: '2026-10-29T11:00:02Z',
    };
    const waiting = aiReducer(initialAiState, { type: 'progress_received', progress: approval });
    const reconnected = { ...target, generation: 2 };
    const next = aiReducer(waiting, {
      type: 'target_changed', target: reconnected,
      conversation: {
        messages: [{ id: 'answer-1', role: 'assistant', kind: 'analysis', text: 'Docker 已安装。' }],
        plans: [{ ...plan, state: 'completed' }],
        executions: { 'execution-1': execution },
        error: '上次网络错误',
      },
    });

    expect(next.messages[0].text).toBe('Docker 已安装。');
    expect(next.plans[0].state).toBe('completed');
    expect(next.executions['execution-1'].stdout).toBe('Docker version 27.0.0\n');
    expect(next.error).toBe('上次网络错误');
    expect(next.plan).toBeNull();
    expect(next.pendingApproval).toBeNull();
    expect(next.waitingForProvider).toBe(false);
    expect(next.running).toBe(false);
  });

  it('provider failure does not affect local terminal state or invent a result', () => {
    const next = aiReducer(initialAiState, { type: 'provider_failed', message: 'AI unavailable' });
    expect(next.error).toBe('AI unavailable');
    expect(next.plan).toBeNull();
    expect(next.pendingApproval).toBeNull();
    expect(next.executions).toEqual({});
    expect(next.waitingForProvider).toBe(false);
  });
});
