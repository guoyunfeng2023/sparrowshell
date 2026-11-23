import type { AiPlan, ExecutionMode, ExecutionRecord, PendingApproval, StepProgress, TargetRef } from '../../types';

export interface AiMessage {
  id: string;
  role: 'user' | 'assistant';
  kind?: 'analysis' | 'plan';
  planId?: string;
  text: string;
}

export interface AiWorkbenchState {
  mode: ExecutionMode;
  target: TargetRef | null;
  messages: AiMessage[];
  plan: AiPlan | null;
  plans: AiPlan[];
  pendingApproval: PendingApproval | null;
  executions: Record<string, ExecutionRecord>;
  waitingForProvider: boolean;
  running: boolean;
  error: string;
}

export const initialAiState: AiWorkbenchState = {
  mode: 'auto_except_high_risk',
  target: null,
  messages: [],
  plan: null,
  plans: [],
  pendingApproval: null,
  executions: {},
  waitingForProvider: false,
  running: false,
  error: '',
};

export type AiRestoredConversation = Pick<AiWorkbenchState, 'messages' | 'plans' | 'executions' | 'error'>;

export type AiAction =
  | { type: 'mode_changed'; mode: ExecutionMode }
  | { type: 'target_changed'; target: TargetRef | null; conversation?: AiRestoredConversation }
  | { type: 'prompt_sent'; id: string; text: string; target: TargetRef }
  | { type: 'provider_plan_received'; message: AiMessage; plan: AiPlan }
  | { type: 'progress_received'; progress: StepProgress }
  | { type: 'analysis_received'; message: AiMessage; target: TargetRef }
  | { type: 'provider_failed'; message: string }
  | { type: 'error_cleared' };

function sameTarget(left: TargetRef | null, right: TargetRef | null) {
  return left?.serverId === right?.serverId && left?.sessionId === right?.sessionId && left?.generation === right?.generation;
}

export function aiReducer(state: AiWorkbenchState, action: AiAction): AiWorkbenchState {
  switch (action.type) {
    case 'mode_changed':
      return { ...state, mode: action.mode };
    case 'target_changed':
      if (sameTarget(state.target, action.target)) return state;
      return {
        ...initialAiState,
        mode: state.mode,
        target: action.target,
        messages: action.conversation?.messages ?? [],
        plans: action.conversation?.plans ?? [],
        executions: action.conversation?.executions ?? {},
        error: action.conversation?.error ?? '',
      };
    case 'prompt_sent':
      return {
        ...state,
        target: action.target,
        messages: [...state.messages, { id: action.id, role: 'user', text: action.text }],
        plan: null,
        plans: [],
        pendingApproval: null,
        executions: {},
        waitingForProvider: true,
        running: false,
        error: '',
      };
    case 'provider_plan_received':
      if (state.target && !sameTarget(state.target, action.plan.target)) return state;
      return {
        ...state,
        messages: [...state.messages, { ...action.message, planId: action.plan.planId }],
        plan: action.plan,
        plans: [...state.plans.filter((plan) => plan.planId !== action.plan.planId), action.plan],
        pendingApproval: null,
        waitingForProvider: false,
        error: '',
      };
    case 'progress_received': {
      if (state.target && !sameTarget(state.target, action.progress.plan.target)) return state;
      const { progress } = action;
      const executions = progress.execution
        ? { ...state.executions, [progress.execution.executionId]: progress.execution }
        : state.executions;
      const plans = state.plans.some((plan) => plan.planId === progress.plan.planId)
        ? state.plans.map((plan) => plan.planId === progress.plan.planId ? progress.plan : plan)
        : [...state.plans, progress.plan];
      return {
        ...state,
        plan: progress.plan,
        plans,
        pendingApproval: progress.approval,
        executions,
        waitingForProvider: false,
        running: progress.plan.state === 'running' || progress.plan.state === 'stopping',
        error: '',
      };
    }
    case 'analysis_received':
      if (!sameTarget(state.target, action.target)) return state;
      return { ...state, messages: [...state.messages, action.message], waitingForProvider: false };
    case 'provider_failed':
      return { ...state, waitingForProvider: false, running: false, error: action.message };
    case 'error_cleared':
      return { ...state, error: '' };
  }
}
