import type { AiConversationMessage, ExecutionRecord, TargetRef } from '../../types';
import type { AiRestoredConversation } from './aiSession';
import type { DynamicAgentSession, DynamicAgentState } from './dynamicAiAgent';

export type AnalysisState = { status: 'loading' | 'success' | 'error'; text: string };

export interface AiCachedConversation {
  conversation: AiRestoredConversation;
  executions: Record<string, ExecutionRecord>;
  analyses: Record<string, AnalysisState>;
  dynamicSession: DynamicAgentSession | null;
  dynamicHistory: AiConversationMessage[];
  qwenSessionId: string | null;
  safeStopReason: DynamicAgentState | null;
}

export function aiConversationKey(target: TargetRef) {
  return `${target.serverId}:${target.sessionId}`;
}

/** Runtime-only cache. A new app process always starts with an empty map. */
export class AiConversationCache {
  private readonly conversations = new Map<string, AiCachedConversation>();

  read(target: TargetRef) {
    return this.conversations.get(aiConversationKey(target));
  }

  write(target: TargetRef, conversation: AiCachedConversation) {
    this.conversations.set(aiConversationKey(target), conversation);
  }
}
