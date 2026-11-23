import type { AiPlan, AiPlanState, ExecutionMode, ExecutionRecord, StepProgress, TargetRef } from '../../types';

export interface AiAgentOptions {
  plan: AiPlan;
  target: TargetRef;
  mode: ExecutionMode;
  runNext: (planId: string, target: TargetRef, mode: ExecutionMode) => Promise<StepProgress>;
  isCurrent: () => boolean;
  onProgress: (progress: StepProgress) => void;
  getExecution: (executionId: string) => ExecutionRecord | undefined;
  shouldAnalyze: (executionId: string) => boolean;
  canAnalyze: () => boolean;
  onAnalysisStart: (executionId: string) => void;
  onAnalysis: (executionId: string, text: string) => void | Promise<void>;
  onAnalysisError: (executionId: string, message: string) => void;
  onAnalysisDenied: (executionId: string) => void;
  analyze: (executionId: string) => Promise<{ executionId: string; analysis: string }>;
  /** Dynamic agents disable per-step analysis and request one final decision instead. */
  analyzeResults?: boolean;
}

export interface AiAgentResult {
  state: AiPlanState | 'stale';
  progress: StepProgress;
}

async function analyzeCompletedPlan(options: AiAgentOptions, currentPlan: AiPlan) {
  for (const step of currentPlan.steps) {
    if (!options.isCurrent()) return;
    const id = step.executionId;
    if (!id || !options.shouldAnalyze(id)) continue;
    const record = options.getExecution(id);
    if (!record || !['succeeded', 'failed'].includes(record.state)) continue;
    if (!options.canAnalyze()) {
      options.onAnalysisDenied(id);
      continue;
    }
    options.onAnalysisStart(id);
    try {
      const response = await options.analyze(id);
      if (!options.isCurrent()) return;
      await options.onAnalysis(id, response.analysis);
    } catch (reason) {
      if (!options.isCurrent()) return;
      options.onAnalysisError(id, reason instanceof Error ? reason.message : String(reason));
    }
  }
}

/** Runs the fixed plan to a safe stopping point, then answers from real results. */
export async function runAiPlan(options: AiAgentOptions, initialProgress?: StepProgress): Promise<AiAgentResult> {
  let currentProgress = initialProgress ?? { plan: options.plan, approval: null, execution: null };
  let currentPlan = currentProgress.plan;

  while (options.isCurrent()) {
    if (currentPlan.state === 'ready') {
      currentProgress = await options.runNext(currentPlan.planId, options.target, options.mode);
      if (!options.isCurrent()) return { state: 'stale', progress: currentProgress };
      currentPlan = currentProgress.plan;
    }

    options.onProgress(currentProgress);

    if (currentPlan.state === 'ready') continue;
    if (options.analyzeResults !== false && (currentPlan.state === 'completed' || currentPlan.state === 'failed')) {
      await analyzeCompletedPlan(options, currentPlan);
    }
    return { state: options.isCurrent() ? currentPlan.state : 'stale', progress: currentProgress };
  }

  return { state: 'stale', progress: currentProgress };
}
