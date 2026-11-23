import { describe, expect, it, vi } from 'vitest';
import type { AiPlan, ExecutionRecord, ExecutionState, StepProgress, TargetRef } from '../../types';
import { runAiPlan } from './aiAgent';

const target: TargetRef = { serverId: 'server-1', sessionId: 'session-1', generation: 1 };

function execution(id: string, state: ExecutionState = 'succeeded'): ExecutionRecord {
  return {
    executionId: id, source: 'ai', target, mode: 'auto_except_high_risk', command: `command-${id}`,
    commandHash: `hash-${id}`, risk: { level: 'low', reasons: [] }, state, approvalId: null,
    approvalExpiresAt: null, stdout: `stdout-${id}`, stderr: '', exitStatus: state === 'succeeded' ? 0 : 1,
    errorCode: null, createdAt: '2026-10-31T00:00:00Z', startedAt: null, finishedAt: null,
  };
}

function plan(states: ExecutionState[], state: AiPlan['state']): AiPlan {
  return {
    planId: 'plan-1', target, state, currentStep: state === 'ready' ? states.length : null,
    steps: states.map((stepState, index) => ({
      proposal: { command: `command-e${index + 1}`, modelRisk: null },
      state: stepState === 'succeeded' ? 'succeeded' : stepState === 'failed' ? 'failed' : 'queued',
      executionId: ['succeeded', 'failed'].includes(stepState) ? `e${index + 1}` : null,
      approvalId: null,
    })),
  };
}

function progress(currentPlan: AiPlan, record: ExecutionRecord | null = null): StepProgress {
  return { plan: currentPlan, approval: null, execution: record };
}

function setup(records: ExecutionRecord[]) {
  const byId = Object.fromEntries(records.map((record) => [record.executionId, record]));
  return {
    target, mode: 'auto_except_high_risk' as const,
    isCurrent: () => true,
    onProgress: vi.fn(),
    getExecution: (id: string) => byId[id],
    shouldAnalyze: () => true,
    canAnalyze: () => true,
    onAnalysisStart: vi.fn(), onAnalysis: vi.fn(), onAnalysisError: vi.fn(), onAnalysisDenied: vi.fn(),
    analyze: vi.fn(async (id: string) => ({ executionId: id, analysis: `answer-${id}` })),
  };
}

describe('AI execution-answer loop', () => {
  it('runs every ready step and analyzes the real results only after completion', async () => {
    const first = execution('e1');
    const second = execution('e2');
    const options = setup([first, second]);
    const runNext = vi.fn()
      .mockResolvedValueOnce(progress(plan(['succeeded'], 'ready'), first))
      .mockResolvedValueOnce(progress(plan(['succeeded', 'succeeded'], 'completed'), second));
    const events: string[] = [];
    options.onAnalysis.mockImplementation(async (id: string) => { events.push(`analyze:${id}`); });
    options.onProgress.mockImplementation((value: StepProgress) => { events.push(`progress:${value.plan.state}`); });

    const result = await runAiPlan({ ...options, plan: plan([], 'ready'), runNext });

    expect(result.state).toBe('completed');
    expect(runNext).toHaveBeenCalledTimes(2);
    expect(events).toEqual(['progress:ready', 'progress:completed', 'analyze:e1', 'analyze:e2']);
    expect(options.analyze.mock.calls.map(([id]) => id)).toEqual(['e1', 'e2']);
  });

  it('pauses for approval and resumes remaining steps from the approved progress', async () => {
    const first = execution('e1');
    const options = setup([first]);
    const waitingPlan = plan([], 'awaiting_approval');
    const waiting = await runAiPlan({
      ...options, plan: plan([], 'ready'),
      runNext: vi.fn().mockResolvedValue(progress(waitingPlan)),
    });
    expect(waiting.state).toBe('awaiting_approval');
    expect(options.analyze).not.toHaveBeenCalled();

    const resumed = await runAiPlan({
      ...options, plan: waitingPlan, runNext: vi.fn(),
    }, progress(plan(['succeeded'], 'completed'), first));
    expect(resumed.state).toBe('completed');
    expect(options.analyze).toHaveBeenCalledWith('e1');
  });

  it('stops consuming an old plan immediately after the target changes', async () => {
    const first = execution('e1');
    let current = true;
    const options = { ...setup([first]), isCurrent: () => current };
    const runNext = vi.fn().mockImplementation(async () => {
      current = false;
      return progress(plan(['succeeded'], 'ready'), first);
    });
    const result = await runAiPlan({ ...options, plan: plan([], 'ready'), runNext });
    expect(result.state).toBe('stale');
    expect(runNext).toHaveBeenCalledTimes(1);
    expect(options.onProgress).not.toHaveBeenCalled();
    expect(options.analyze).not.toHaveBeenCalled();
  });

  it('never sends results to analysis without recent-results permission', async () => {
    const record = execution('e1');
    const options = { ...setup([record]), canAnalyze: () => false };
    await runAiPlan({ ...options, plan: plan([], 'ready'), runNext: vi.fn() }, progress(plan(['succeeded'], 'completed'), record));
    expect(options.analyze).not.toHaveBeenCalled();
    expect(options.onAnalysisDenied).toHaveBeenCalledWith('e1');
  });

  it('keeps successful command progress when analysis fails', async () => {
    const record = execution('e1');
    const options = setup([record]);
    options.analyze.mockRejectedValue(new Error('analysis unavailable'));
    const result = await runAiPlan({ ...options, plan: plan([], 'ready'), runNext: vi.fn() }, progress(plan(['succeeded'], 'completed'), record));
    expect(result.state).toBe('completed');
    expect(options.onProgress).toHaveBeenCalledTimes(1);
    expect(options.onAnalysisError).toHaveBeenCalledWith('e1', 'analysis unavailable');
  });

  it('does not analyze stopped, disconnected, unknown, or unrelated execution records', async () => {
    const unrelated = execution('other');
    for (const state of ['stopped', 'paused_disconnected', 'result_unknown'] as const) {
      const options = setup([unrelated]);
      await runAiPlan({ ...options, plan: plan([], state), runNext: vi.fn() }, progress(plan([], state)));
      expect(options.analyze).not.toHaveBeenCalled();
      expect(options.onAnalysisDenied).not.toHaveBeenCalled();
    }
  });

  it('lets dynamic agents disable duplicate per-step analysis', async () => {
    const record = execution('e1');
    const options = setup([record]);
    const result = await runAiPlan({ ...options, plan: plan([], 'ready'), analyzeResults: false, runNext: vi.fn() }, progress(plan(['succeeded'], 'completed'), record));
    expect(result.state).toBe('completed');
    expect(options.analyze).not.toHaveBeenCalled();
    expect(options.onAnalysisDenied).not.toHaveBeenCalled();
  });

  it('skips records already analyzed manually', async () => {
    const record = execution('e1');
    const options = { ...setup([record]), shouldAnalyze: () => false };
    await runAiPlan({ ...options, plan: plan([], 'ready'), runNext: vi.fn() }, progress(plan(['succeeded'], 'completed'), record));
    expect(options.analyze).not.toHaveBeenCalled();
    expect(options.onAnalysisDenied).not.toHaveBeenCalled();
  });
});
