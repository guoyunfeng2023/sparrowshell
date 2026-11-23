import { useEffect, useState } from 'react';
import { LoaderCircle } from 'lucide-react';
import type { MessageKey } from '../../i18n';
import type { ConnectionState } from '../../types';

type Translate = (key: MessageKey) => string;

export function isConnectingState(state?: ConnectionState | null) {
  return state === 'resolving' || state === 'verifying_target' || state === 'authenticating';
}

export function connectionPhaseKey(state?: ConnectionState | null): MessageKey {
  if (state === 'authenticating') return 'connectPhaseAuth';
  if (isConnectingState(state)) return 'connectPhaseNetwork';
  return 'connectPhasePrepare';
}

export function ConnectionFeedback({ serverName, state, t }: { serverName: string; state?: ConnectionState | null; t: Translate }) {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => setElapsed((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, []);

  return <div className="welcome connection-welcome">
    <div className="welcome-icon connection-icon"><LoaderCircle className="spin" size={28} /></div>
    <h1>{t('connecting')} {serverName}</h1>
    <p role="status" aria-live="polite">{t(connectionPhaseKey(state))}</p>
    <div className="connection-progress" role="progressbar" aria-label={t('connecting')}><span /></div>
    <small><span aria-hidden="true">{t('connectElapsed').replace('{seconds}', String(elapsed))} · </span>{t('connectWaitHint')}</small>
  </div>;
}
