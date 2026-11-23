import { describe, expect, it, vi } from 'vitest';
import type { AiAgentTurnResult, AiPlan, ExecutionRecord, ExecutionState, StepProgress, TargetRef } from '../../types';
import { runDynamicAgent } from './dynamicAiAgent';
import type { DynamicAgentOptions, DynamicAgentSession } from './dynamicAiAgent';

const target: TargetRef = { serverId: 'server-1', sessionId: 'session-1', generation: 1 };

function plan(id: string, command: string, state: AiPlan['state'] = 'ready'): AiPlan {
  return {
    planId: id, target, state, currentStep: state === 'ready' ? 0 : null,
    steps: [{ proposal: { command, modelRisk: null }, state: 'queued', executionId: null, approvalId: null }],
  };
}

function execution(id: string, command: string, state: ExecutionState = 'succeeded'): ExecutionRecord {
  return {
    executionId: id, source: 'ai', target, mode: 'auto_except_high_risk', command,
    commandHash: `hash-${id}`, risk: { level: 'low', reasons: [] }, state, approvalId: null,
    approvalExpiresAt: null, stdout: state === 'succeeded' ? '/usr/bin/docker' : '', stderr: state === 'failed' ? 'not found' : '',
    exitStatus: state === 'succeeded' ? 0 : 1, errorCode: null,
    createdAt: '2026-11-01T00:00:00Z', startedAt: null, finishedAt: null,
  };
}

function progress(currentPlan: AiPlan, record: ExecutionRecord | null = null): StepProgress {
  return { plan: currentPlan, approval: null, execution: record };
}

function tool(command: string, id = `plan-${command}`): AiAgentTurnResult {
  return { type: 'tool_call', summary: `执行 ${command}`, plan: plan(id, command), sessionId: 'qwen-session-1' };
}

function answer(text: string): AiAgentTurnResult {
  return { type: 'final_answer', answer: text, sessionId: 'qwen-session-1' };
}

function setup(overrides: Partial<DynamicAgentOptions> = {}) {
  const options: DynamicAgentOptions = {
    intent: '检查 Docker', taskId: 'd2d5bafa-76ce-4a72-9db7-a82f527dd17b', target, mode: 'auto_except_high_risk',
    isCurrent: () => true, canSendResults: () => true,
    requestTurn: vi.fn(), runPlan: vi.fn(), onPlan: vi.fn(), onFinalAnswer: vi.fn(),
    ...overrides,
  };
  return options;
}

function completed(currentPlan: AiPlan, record: ExecutionRecord) {
  return { state: 'completed' as const, progress: progress({ ...currentPlan, state: 'completed' }, record) };
}

describe('dynamic multi-step AI agent', () => {
  it('answers ordinary questions without fabricating a command plan', async () => {
    const options = setup({ requestTurn: vi.fn().mockResolvedValue(answer('Docker 是容器平台。')) });
    const result = await runDynamicAgent(options);
    expect(result.state).toBe('final_answer');
    expect(options.onFinalAnswer).toHaveBeenCalledWith('Docker 是容器平台。');
    expect(options.runPlan).not.toHaveBeenCalled();
    expect(result.session.executionIds).toEqual([]);
  });

  it('uses a failed real result to choose the next command instead of claiming success', async () => {
    const first = plan('p1', 'command -v docker');
    const failed = execution('e1', 'command -v docker', 'failed');
    const requestTurn = vi.fn()
      .mockResolvedValueOnce(tool('command -v docker', 'p1'))
      .mockResolvedValueOnce(answer('没有找到 Docker CLI，建议继续检查软件包。'));
    const options = setup({ requestTurn, runPlan: vi.fn().mockResolvedValue(completed(first, failed)) });
    const result = await runDynamicAgent(options);
    expect(requestTurn).toHaveBeenNthCalledWith(2, ['e1']);
    expect(result.state).toBe('final_answer');
    expect(result.session.commands).toEqual(['command -v docker']);
  });

  it('runs multiple decisions and finishes from all accumulated execution ids', async () => {
    const first = plan('p1', 'command -v docker');
    const second = plan('p2', 'systemctl is-active docker');
    const requestTurn = vi.fn()
      .mockResolvedValueOnce(tool('command -v docker', 'p1'))
      .mockResolvedValueOnce(tool('systemctl is-active docker', 'p2'))
      .mockResolvedValueOnce(answer('已安装，服务正在运行。'));
    const runPlan = vi.fn()
      .mockResolvedValueOnce(completed(first, execution('e1', 'command -v docker')))
      .mockResolvedValueOnce(completed(second, execution('e2', 'systemctl is-active docker')));
    const result = await runDynamicAgent(setup({ requestTurn, runPlan }));
    expect(requestTurn.mock.calls.map(([ids]) => ids)).toEqual([[], ['e1'], ['e1', 'e2']]);
    expect(result.session.executionIds).toEqual(['e1', 'e2']);
    expect(result.state).toBe('final_answer');
  });

  it('pauses at approval and resumes from real approved progress without rerunning the command', async () => {
    const waitingPlan = plan('p1', 'rm -rf /tmp/demo', 'awaiting_approval');
    const waitingProgress = { plan: waitingPlan, approval: { approvalId: 'a1' }, execution: null } as StepProgress;
    const options = setup({
      requestTurn: vi.fn().mockResolvedValue(tool('rm -rf /tmp/demo', 'p1')),
      runPlan: vi.fn().mockResolvedValue({ state: 'awaiting_approval' as const, progress: waitingProgress }),
    });
    const waiting = await runDynamicAgent(options);
    expect(waiting.state).toBe('awaiting_approval');
    expect(waiting.session.executionIds).toEqual([]);

    const record = execution('e1', 'rm -rf /tmp/demo');
    const resumedOptions = setup({
      session: waiting.session,
      requestTurn: vi.fn().mockResolvedValue(answer('命令已执行完成。')),
      runPlan: vi.fn(),
    });
    const resumed = await runDynamicAgent(resumedOptions, progress({ ...waitingPlan, state: 'completed' }, record));
    expect(resumedOptions.runPlan).not.toHaveBeenCalled();
    expect(resumedOptions.requestTurn).toHaveBeenCalledWith(['e1']);
    expect(resumed.state).toBe('final_answer');
  });

  it('permits one final decision after the default twenty commands but rejects another command', async () => {
    const requestTurn = vi.fn();
    const runPlan = vi.fn();
    for (let index = 1; index <= 20; index++) {
      const current = plan(`p${index}`, `command-${index}`);
      requestTurn.mockResolvedValueOnce(tool(`command-${index}`, `p${index}`));
      runPlan.mockResolvedValueOnce(completed(current, execution(`e${index}`, `command-${index}`)));
    }
    requestTurn.mockResolvedValueOnce(tool('command-21', 'p21'));
    const result = await runDynamicAgent(setup({ requestTurn, runPlan }));
    expect(runPlan).toHaveBeenCalledTimes(20);
    expect(requestTurn).toHaveBeenCalledTimes(21);
    expect(result.state).toBe('step_limit');
    expect(result.session.commands).toHaveLength(20);
  });

  it('honors the configured per-task command limit', async () => {
    const requestTurn = vi.fn();
    const runPlan = vi.fn();
    for (let index = 1; index <= 2; index++) {
      const current = plan(`p${index}`, `command-${index}`);
      requestTurn.mockResolvedValueOnce(tool(`command-${index}`, `p${index}`));
      runPlan.mockResolvedValueOnce(completed(current, execution(`e${index}`, `command-${index}`)));
    }
    requestTurn.mockResolvedValueOnce(tool('command-3', 'p3'));
    const result = await runDynamicAgent(setup({ requestTurn, runPlan, maxSteps: 2 }));
    expect(runPlan).toHaveBeenCalledTimes(2);
    expect(requestTurn).toHaveBeenCalledTimes(3);
    expect(result.state).toBe('step_limit');
    expect(result.session.commands).toHaveLength(2);
  });

  it('stops before executing a consecutive duplicate command', async () => {
    const first = plan('p1', 'docker info');
    const requestTurn = vi.fn()
      .mockResolvedValueOnce(tool('docker info', 'p1'))
      .mockResolvedValueOnce(tool('docker info', 'p2'));
    const options = setup({ requestTurn, runPlan: vi.fn().mockResolvedValue(completed(first, execution('e1', 'docker info'))) });
    const result = await runDynamicAgent(options);
    expect(result.state).toBe('repeated_command');
    expect(options.runPlan).toHaveBeenCalledTimes(1);
    expect(result.session.executionIds).toEqual(['e1']);
  });

  it.each(['stopped', 'paused_disconnected', 'result_unknown'] as const)('does not continue after %s', async (state) => {
    const current = plan('p1', 'docker info');
    const options = setup({
      requestTurn: vi.fn().mockResolvedValue(tool('docker info', 'p1')),
      runPlan: vi.fn().mockResolvedValue({ state, progress: progress({ ...current, state }) }),
    });
    const result = await runDynamicAgent(options);
    expect(result.state).toBe(state);
    expect(options.requestTurn).toHaveBeenCalledTimes(1);
  });

  it('requires explicit recent-results permission before the follow-up decision', async () => {
    const first = plan('p1', 'docker info');
    const options = setup({
      canSendResults: () => false,
      requestTurn: vi.fn().mockResolvedValue(tool('docker info', 'p1')),
      runPlan: vi.fn().mockResolvedValue(completed(first, execution('e1', 'docker info'))),
    });
    const result = await runDynamicAgent(options);
    expect(result.state).toBe('permission_required');
    expect(options.requestTurn).toHaveBeenCalledTimes(1);
  });

  it('drops late model decisions after target change or stop', async () => {
    let current = true;
    const options = setup({
      isCurrent: () => current,
      requestTurn: vi.fn().mockImplementation(async () => { current = false; return tool('docker info'); }),
    });
    const result = await runDynamicAgent(options);
    expect(result.state).toBe('stale');
    expect(options.runPlan).not.toHaveBeenCalled();
    expect(options.onPlan).not.toHaveBeenCalled();
  });

  it('propagates model failures without inventing execution progress', async () => {
    const options = setup({ requestTurn: vi.fn().mockRejectedValue(new Error('model unavailable')) });
    await expect(runDynamicAgent(options)).rejects.toThrow('model unavailable');
    expect(options.runPlan).not.toHaveBeenCalled();
  });

  it('rejects malformed resumed progress instead of duplicating execution', async () => {
    const session: DynamicAgentSession = { intent: '检查', taskId: 'd2d5bafa-76ce-4a72-9db7-a82f527dd17b', target, executionIds: [], commands: [], plan: plan('p1', 'true') };
    const options = setup({ session });
    const result = await runDynamicAgent(options, progress(plan('other', 'true', 'completed'), execution('e1', 'true')));
    expect(result.state).toBe('stale');
    expect(options.requestTurn).not.toHaveBeenCalled();
  });
});
