import { useEffect, useMemo, useRef, useState } from 'react';
import { Activity, ChevronRight, RefreshCw, Server, ShieldAlert, X } from 'lucide-react';
import { desktop, isDesktop } from '../../ipc/client';
import type { MessageKey } from '../../i18n';
import type { ProcessInfo, ProcessSignal, SessionSnapshot, SystemSnapshot } from '../../types';
import { formatBytes, formatUptime } from './format';
import { diskUsagePercent, selectDisks, sortDisksByUsage } from './diskUsage';
import { systemContext } from '../ai/context';

type Translate = (key: MessageKey) => string;
type ProcessView = 'cpu' | 'memory' | null;

function MetricRing({ value, label, detail, tone = 'green', onClick, actionLabel }: { value: number; label: string; detail: string; tone?: 'green' | 'blue'; onClick: () => void; actionLabel: string }) {
  const radius = 24;
  const circumference = 2 * Math.PI * radius;
  const normalized = Math.min(100, Math.max(0, value));
  return <button type="button" className="metric-ring-card" onClick={onClick} aria-label={actionLabel}>
    <ChevronRight size={12} className="metric-open-icon" />
    <div className={`metric-ring ${tone}`}>
      <svg viewBox="0 0 60 60" aria-hidden="true"><circle className="ring-track" cx="30" cy="30" r={radius} /><circle className="ring-value" cx="30" cy="30" r={radius} strokeDasharray={circumference} strokeDashoffset={circumference * (1 - normalized / 100)} /></svg>
      <strong>{Math.round(normalized)}<small>%</small></strong>
    </div>
    <span className="metric-label">{label}</span>
    <small className="metric-detail">{detail}</small>
  </button>;
}

export function MonitorPanel({ session, serverName, host, username, locale, t, onSnapshot }: { session: SessionSnapshot; serverName: string; host: string; username: string; locale: 'zh' | 'en'; t: Translate; onSnapshot: (context: string) => void }) {
  const [snapshot, setSnapshot] = useState<SystemSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [stale, setStale] = useState(false);
  const [error, setError] = useState('');
  const [processView, setProcessView] = useState<ProcessView>(null);
  const [showAllMounts, setShowAllMounts] = useState(false);
  const [action, setAction] = useState('');
  const [refreshInterval, setRefreshInterval] = useState(5);
  const sequence = useRef(0);

  const refresh = async () => {
    if (!isDesktop) return;
    const request = ++sequence.current;
    setLoading(true);
    try {
      const next = await desktop.monitorSnapshot(session.target.sessionId, session.target.generation);
      if (request !== sequence.current) return;
      setSnapshot(next);
      onSnapshot(systemContext(next));
      setStale(false);
      setError('');
    } catch (reason) {
      if (request !== sequence.current) return;
      setStale(true);
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (request === sequence.current) setLoading(false);
    }
  };

  useEffect(() => {
    setSnapshot(null);
    setError('');
    setStale(false);
    setProcessView(null);
  }, [session.target.sessionId, session.target.generation]);

  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let running = false;
    const tick = async () => {
      if (running || disposed) return;
      running = true;
      await refresh();
      running = false;
    };
    void tick();
    const timer = window.setInterval(() => void tick(), refreshInterval * 1000);
    return () => {
      disposed = true;
      sequence.current += 1;
      window.clearInterval(timer);
    };
  }, [session.target.sessionId, session.target.generation, refreshInterval]);

  const processes = useMemo(() => {
    if (!snapshot || !processView) return [];
    const key = processView === 'cpu' ? 'cpuPercent' : 'memoryPercent';
    return [...snapshot.processes].sort((left, right) => right[key] - left[key]).slice(0, 100);
  }, [snapshot, processView]);

  const terminate = async (process: ProcessInfo, signal: ProcessSignal) => {
    const actionKey = `${process.pid}-${signal}`;
    const message = `${t(signal === 'kill' ? 'confirmKill' : 'confirmTerminate')}\n\n${serverName}\nPID ${process.pid} · ${process.user}\n${process.command}`;
    if (!window.confirm(message)) return;
    setAction(actionKey);
    setError('');
    try {
      await desktop.terminateProcess(session.target.sessionId, session.target.generation, process.pid, process.startId, signal);
      setProcessView(null);
      await refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setAction('');
    }
  };

  const memoryPercent = snapshot ? snapshot.memoryUsedBytes / snapshot.memoryTotalBytes * 100 : 0;
  const cpuCores = snapshot?.cpuCores;
  const cpuCoreLabel = typeof cpuCores === 'number' && Number.isFinite(cpuCores) && cpuCores > 0
    ? `${cpuCores} ${locale === 'zh' ? '核' : 'cores'}`
    : locale === 'zh' ? '核数未知' : 'Unknown cores';
  return <section className="monitor-panel">
    <div className="panel-heading"><span><Activity size={14} />{t('monitor')}</span><div className="monitor-actions"><select title={t('refreshInterval')} aria-label={t('refreshInterval')} value={refreshInterval} onChange={(event) => setRefreshInterval(Number(event.target.value))}>{[5, 15, 30, 60].map((seconds) => <option key={seconds} value={seconds}>{locale === 'zh' ? `${seconds} 秒` : `${seconds}s`}</option>)}</select><button className="icon small" title={t('refresh')} disabled={loading || !isDesktop} onClick={() => void refresh()}><RefreshCw size={13} className={loading ? 'spin' : ''} /></button></div></div>
    {!isDesktop ? <div className="ops-empty">{t('monitorPreview')}</div> : !snapshot ? <div className="ops-empty">{loading ? t('collecting') : t('noMetrics')}</div> : <>
      <div className="host-card"><div className="host-icon"><Server size={20} /></div><div className="host-identity"><strong>{serverName}</strong><span>{snapshot.osName}</span></div><span className="live-badge"><i className={`status-dot ${session.state === 'connected' ? 'online' : ''}`} />{t(session.state === 'connected' ? 'online' : 'disconnected')}</span></div>
      <div className="host-meta"><span>{t('host')}<b title={host}>{host}</b></span><span>{t('runningTime')}<b>{formatUptime(snapshot.uptimeSeconds)}</b></span><span>{t('systemArchitecture')}<b>{snapshot.architecture} · {cpuCoreLabel}</b></span><span>{t('currentUser')}<b>{username}</b></span><span className="host-meta-kernel">{t('kernelVersion')}<b title={snapshot.kernel}>{snapshot.kernel}</b></span></div>
      {stale && <div className="stale-banner">{t('stale')}</div>}
      <div className="metrics-grid"><MetricRing value={snapshot.cpuUsagePercent} label={t('cpu')} detail={`${cpuCoreLabel} · ${t('load')} ${snapshot.loadAverage[0].toFixed(2)}`} actionLabel={t('cpuProcesses')} onClick={() => setProcessView('cpu')} /><MetricRing value={memoryPercent} label={t('memory')} detail={`${formatBytes(snapshot.memoryUsedBytes)} / ${formatBytes(snapshot.memoryTotalBytes)}`} tone="blue" actionLabel={t('memoryProcesses')} onClick={() => setProcessView('memory')} /></div>
      <div className="disk-view-switch" role="group" aria-label={t('diskView')}><button type="button" className={!showAllMounts ? 'active' : ''} aria-pressed={!showAllMounts} onClick={() => setShowAllMounts(false)}>{t('diskPartitions')}</button><button type="button" className={showAllMounts ? 'active' : ''} aria-pressed={showAllMounts} onClick={() => setShowAllMounts(true)}>{t('allMountPoints')}</button></div>
      <div className="disk-list">{sortDisksByUsage(selectDisks(snapshot.disks, showAllMounts)).map((disk) => <div key={`${disk.filesystem}:${disk.mountPoint}`}><span className="disk-location"><span title={disk.mountPoint}>{disk.mountPoint}</span>{showAllMounts && <small title={disk.filesystem}>{disk.filesystem}</small>}</span><b>{formatBytes(disk.usedBytes)} / {formatBytes(disk.totalBytes)}</b><i><em style={{ width: `${diskUsagePercent(disk)}%` }} /></i></div>)}</div>
      <div className="collected-at">{t('collectedAt')} {new Date(snapshot.collectedAt).toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US')}</div>
    </>}
    {error && <div className="ops-error" role="alert">{error}</div>}
    {processView && snapshot && <div className="modal-backdrop"><section className="modal process-modal" role="dialog" aria-modal="true" aria-label={t(processView === 'cpu' ? 'cpuProcesses' : 'memoryProcesses')}>
      <header><h2>{t(processView === 'cpu' ? 'cpuProcesses' : 'memoryProcesses')}</h2><button className="icon" onClick={() => setProcessView(null)} aria-label={t('cancel')}><X size={16} /></button></header>
      <div className="process-context"><ShieldAlert size={15} />{t('processSafety')}</div>
      <div className="process-table"><div className="process-row process-header"><span>PID</span><span>{t('user')}</span><span>CPU</span><span>{t('memory')}</span><span>{t('command')}</span><span>{t('actions')}</span></div>
        {processes.map((process) => <div className="process-row" key={`${process.pid}-${process.startId}`} title={`${t('started')} ${process.startId}\n${process.command}`}><span>{process.pid}</span><span>{process.user}</span><span>{process.cpuPercent.toFixed(1)}%</span><span>{process.memoryPercent.toFixed(1)}%</span><span>{process.command}</span><span className="process-actions"><button disabled={!!action} title={t('terminateNormallyHint')} aria-label={t('terminateNormallyHint')} onClick={() => void terminate(process, 'terminate')}>{action === `${process.pid}-terminate` ? '…' : t('terminateNormally')}</button><button className="kill" disabled={!!action} title={t('terminateForceHint')} aria-label={t('terminateForceHint')} onClick={() => void terminate(process, 'kill')}>{action === `${process.pid}-kill` ? '…' : t('terminateForce')}</button></span></div>)}
      </div>
    </section></div>}
  </section>;
}
