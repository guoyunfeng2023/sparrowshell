import { describe, expect, it } from 'vitest';
import { aiReducer, initialAiState } from './aiSession';
import type { AiPlan, TargetRef } from '../../types';

const oldTarget: TargetRef = { serverId: 'old-server', sessionId: 'old-session', generation: 1 };
const newTarget: TargetRef = { serverId: 'new-server', sessionId: 'new-session', generation: 1 };
const oldPlan: AiPlan = {
  planId: 'old-plan', target: oldTarget, state: 'ready', currentStep: null,
  steps: [{ proposal: { command: 'df -h', modelRisk: null }, state: 'queued', executionId: null, approvalId: null }],
};

describe('AI response target binding', () => {
  it('ignores a late provider response after switching server or generation', () => {
    const switched = aiReducer(initialAiState, { type: 'target_changed', target: newTarget });
    const next = aiReducer(switched, { type: 'provider_plan_received', message: { id: 'old', role: 'assistant', text: '旧响应' }, plan: oldPlan });
    expect(next.target).toEqual(newTarget);
    expect(next.plan).toBeNull();
    expect(next.messages).toEqual([]);
  });

  it('ignores a late analysis answer after switching target', () => {
    const switched = aiReducer(initialAiState, { type: 'target_changed', target: newTarget });
    const next = aiReducer(switched, {
      type: 'analysis_received', target: oldTarget,
      message: { id: 'old-answer', role: 'assistant', kind: 'analysis', text: '旧分析' },
    });
    expect(next.messages).toEqual([]);
  });

  it('does not render late approval or execution results on another target', () => {
    const switched = aiReducer(initialAiState, { type: 'target_changed', target: newTarget });
    const next = aiReducer(switched, { type: 'progress_received', progress: { plan: oldPlan, approval: null, execution: null } });
    expect(next.plan).toBeNull();
    expect(next.pendingApproval).toBeNull();
  });
});
