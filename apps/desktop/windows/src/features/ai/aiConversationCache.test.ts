import { describe, expect, it } from 'vitest';
import type { TargetRef } from '../../types';
import { AiConversationCache } from './aiConversationCache';
import type { AiCachedConversation } from './aiConversationCache';

const target: TargetRef = { serverId: 'server-1', sessionId: 'session-1', generation: 1 };

function conversation(label: string, conversationTarget = target): AiCachedConversation {
  return {
    conversation: {
      messages: [{ id: 'message-1', role: 'assistant', text: label }],
      plans: [],
      executions: {},
      error: '',
    },
    executions: {},
    analyses: {},
    dynamicSession: {
      intent: label,
      taskId: 'd2d5bafa-76ce-4a72-9db7-a82f527dd17b',
      target: conversationTarget,
      executionIds: [],
      commands: [],
      plan: null,
    },
    dynamicHistory: [],
    qwenSessionId: `qwen-${label}`,
    safeStopReason: null,
  };
}

describe('AI runtime conversation cache', () => {
  it('isolates conversations by server and terminal tab', () => {
    const cache = new AiConversationCache();
    const otherServer = { ...target, serverId: 'server-2' };
    const otherTab = { ...target, sessionId: 'session-2' };
    cache.write(target, conversation('server one, tab one'));
    cache.write(otherServer, conversation('server two, tab one', otherServer));

    expect(cache.read(target)?.conversation.messages[0].text).toBe('server one, tab one');
    expect(cache.read(otherServer)?.conversation.messages[0].text).toBe('server two, tab one');
    expect(cache.read(otherTab)).toBeUndefined();
  });

  it('retains the same terminal conversation after reconnecting with a new generation', () => {
    const cache = new AiConversationCache();
    cache.write(target, conversation('before reconnect'));

    expect(cache.read({ ...target, generation: 2 })?.conversation.messages[0].text).toBe('before reconnect');
  });

  it('starts empty in a new app process', () => {
    const cache = new AiConversationCache();
    cache.write(target, conversation('runtime only'));

    const restartedProcess = new AiConversationCache();
    expect(restartedProcess.read(target)).toBeUndefined();
  });
});
