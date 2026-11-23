import type { AiAgentTurnResult, AiPlan, ExecutionMode, StepProgress, TargetRef } from '../../types';
import type { AiAgentResult } from './aiAgent';

export const DEFAULT_DYNAMIC_AGENT_STEPS = 20;
export const MIN_DYNAMIC_AGENT_STEPS = 1;
export const MAX_DYNAMIC_AGENT_STEPS = 50;

export interface DynamicAgentSession {
  intent: string;
  taskId: string;
  target: TargetRef;
  executionIds: string[];
  commands: string[];
  plan: AiPlan | null;
}

export type DynamicAgentState =
  | 'final_answer'
  | 'awaiting_approval'
  | 'stopped'
  | 'paused_disconnected'
  | 'result_unknown'
  | 'permission_required'
  | 'step_limit'
  | 'repeated_command'
  | 'stale';

export interface DynamicAgentOptions {
  intent: string;
  taskId: string;
  target: TargetRef;
  mode: ExecutionMode;
  session?: DynamicAgentSession;
  maxSteps?: number;
  isCurrent: () => boolean;
  canSendResults: () => boolean;
  requestTurn: (executionIds: string[]) => Promise<AiAgentTurnResult>;
  runPlan: (plan: AiPlan) => Promise<AiAgentResult>;
  onPlan: (summary: string, plan: AiPlan) => void;
  onFinalAnswer: (answer: string) => void;
}

export interface DynamicAgentResult {
  state: DynamicAgentState;
  session: DynamicAgentSession;
  progress?: StepProgress;
}

function sameTarget(left: TargetRef, right: TargetRef) {
  return left.serverId === right.serverId
    && left.sessionId === right.sessionId
    && left.generation === right.generation;
}

export function normalizeDynamicAgentSteps(value?: number) {
  if (value === undefined || !Number.isFinite(value)) return DEFAULT_DYNAMIC_AGENT_STEPS;
  return Math.min(MAX_DYNAMIC_AGENT_STEPS, Math.max(MIN_DYNAMIC_AGENT_STEPS, Math.trunc(value)));
}

/** Runs one model decision at a time and only continues from broker-owned terminal results. */
export async function runDynamicAgent(
  options: DynamicAgentOptions,
  initialProgress?: StepProgress,
): Promise<DynamicAgentResult> {
  const previous = options.session;
  const session: DynamicAgentSession = {
    intent: previous?.intent ?? options.intent,
    taskId: previous?.taskId ?? options.taskId,
    target: previous?.target ?? options.target,
    executionIds: [...(previous?.executionIds ?? [])],
    commands: [...(previous?.commands ?? [])],
    plan: previous?.plan ?? null,
  };
  if (!sameTarget(session.target, options.target)) return { state: 'stale', session };
  if (initialProgress && (!session.plan || initialProgress.plan.planId !== session.plan.planId)) {
    return { state: 'stale', session };
  }

  let resumed = initialProgress;
  while (options.isCurrent()) {
    let executionResult: AiAgentResult;
    if (resumed) {
      executionResult = { state: resumed.plan.state, progress: resumed };
      resumed = undefined;
    } else {
      if (session.executionIds.length > 0 && !options.canSendResults()) {
        return { state: 'permission_required', session };
      }
      const decision = await options.requestTurn([...session.executionIds]);
      if (!options.isCurrent()) return { state: 'stale', session };
      if (decision.type === 'final_answer') {
        options.onFinalAnswer(decision.answer);
        return { state: 'final_answer', session };
      }
      if (session.commands.length >= normalizeDynamicAgentSteps(options.maxSteps)) {
        return { state: 'step_limit', session };
      }
      const command = decision.plan.steps[0]?.proposal.command.trim();
      if (!command) return { state: 'result_unknown', session };
      if (session.commands[session.commands.length - 1] === command) {
        return { state: 'repeated_command', session };
      }
      session.plan = decision.plan;
      options.onPlan(decision.summary, decision.plan);
      executionResult = await options.runPlan(decision.plan);
      if (!options.isCurrent()) return { state: 'stale', session };
    }

    const { progress } = executionResult;
    session.plan = progress.plan;
    if (executionResult.state === 'stale') return { state: 'stale', session, progress };
    if (executionResult.state === 'awaiting_approval') {
      return { state: 'awaiting_approval', session, progress };
    }
    if (executionResult.state === 'stopped' || executionResult.state === 'paused_disconnected'
      || executionResult.state === 'result_unknown') {
      return { state: executionResult.state, session, progress };
    }
    if (executionResult.state !== 'completed' && executionResult.state !== 'failed') {
      return { state: 'result_unknown', session, progress };
    }

    const record = progress.execution;
    if (!record || !sameTarget(record.target, options.target)
      || !['succeeded', 'failed'].includes(record.state)) {
      return { state: 'result_unknown', session, progress };
    }
    session.executionIds.push(record.executionId);
    session.commands.push(record.command.trim());
  }

  return { state: 'stale', session };
}
