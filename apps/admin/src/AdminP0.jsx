import React, { useEffect, useState } from 'react';
import { Activity, AlertTriangle, CheckCircle2, CloudCog, Database, Laptop, RefreshCw, ShieldCheck, X } from 'lucide-react';
import { adminRequest, formatTime } from './api';

export function useP0Remote(path, token, onUnauthorized, enabled = true) {
  const [state, setState] = useState({ data: null, loading: enabled, error: '' });
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    if (!enabled) { setState({ data: null, loading: false, error: '' }); return undefined; }
    const controller = new AbortController();
    setState(previous => ({ ...previous, loading: true, error: '' }));
    adminRequest(path, { token, onUnauthorized, signal: controller.signal })
      .then(data => setState({ data, loading: false, error: '' }))
      .catch(error => { if (error.name !== 'AbortError') setState(previous => ({ ...previous, loading: false, error: error.message })); });
    return () => controller.abort();
  }, [path, token, revision, enabled]);
  return { ...state, refresh: () => setRevision(value => value + 1) };
}

export function RangeControl({ value, onChange }) {
  return <div className="segmented" aria-label="统计时间范围">{[['today', '今日'], ['7d', '近 7 天'], ['30d', '近 30 天']].map(([id, label]) => <button key={id} className={value === id ? 'active' : ''} onClick={() => onChange(id)}>{label}</button>)}</div>;
}

function Section({ title, subtitle, children }) {
  return <section className="panel"><header className="panel-header"><div><h2>{title}</h2>{subtitle && <p>{subtitle}</p>}</div></header>{children}</section>;
}
function Problem({ children }) {
  return children ? <div className="inline-notice warning"><AlertTriangle size={16} /><span>{children}</span></div> : null;
}
function None({ text = '当前条件没有真实记录' }) {
  return <div className="empty-state"><Database size={24} /><strong>暂无数据</strong><span>{text}</span></div>;
}
function State({ value }) {
  const success = ['succeeded', 'available', 'completed', 'active', 'valid', 'configured'].includes(value);
  return <span className={`status-badge ${success ? 'green' : ['failed', 'unavailable'].includes(value) ? 'red' : 'gray'}`}><i />{value || '—'}</span>;
}
function Fields({ items }) {
  return <dl>{items.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value ?? '—'}</dd></div>)}</dl>;
}
function Count(value) { return value == null ? '—' : Number(value).toLocaleString('zh-CN'); }

export function UserDetailDrawer({ account, token, onUnauthorized, onClose }) {
  const [tab, setTab] = useState('account');
  const detail = useP0Remote(`/users/${encodeURIComponent(account.id)}/detail`, token, onUnauthorized);
  const devices = useP0Remote(`/devices?limit=200&accountId=${encodeURIComponent(account.id)}`, token, onUnauthorized, tab === 'devices');
  const data = detail.data;
  const tabs = [['account', '账号概览'], ['identity', '登录身份'], ['devices', '设备会话'], ['sync', '同步概况'], ['ai', 'AI 用量'], ['management', '管理记录']];
  return <div className="drawer-layer"><button className="drawer-dismiss" aria-label="关闭用户详情" onClick={onClose} /><aside className="drawer p0-user-drawer" role="dialog" aria-modal="true" aria-label="用户详情"><header className="drawer-header"><div><span className="eyebrow">ACCOUNT DETAIL</span><h2>用户详情</h2></div><button className="icon-btn" aria-label="关闭" onClick={onClose}><X size={18} /></button></header><div className="drawer-profile"><span className="profile-avatar">{(account.nickname || account.email || 'U')[0].toUpperCase()}</span><div><h3>{account.nickname || account.email}</h3><p>{account.id}</p></div><State value={account.status} /></div><nav className="drawer-tabs p0-detail-tabs">{tabs.map(([id, label]) => <button key={id} className={tab === id ? 'active' : ''} onClick={() => setTab(id)}>{label}</button>)}</nav><div className="drawer-content"><Problem>{detail.error}</Problem>{data && tab === 'account' && <div className="detail-section"><h3>账号信息</h3><Fields items={[["账号 ID", data.account.id], ["邮箱", data.account.email], ["昵称", data.account.nickname], ["状态", data.account.status], ["Revision", data.account.revision], ["注册时间", formatTime(data.account.createdAt)], ["最近云端活跃", formatTime(data.account.lastActiveAt)], ["禁用原因", data.account.disabledReason]]} /></div>}{data && tab === 'identity' && <><div className="detail-section"><h3>登录身份</h3><Fields items={[["认证方式", "邮箱密码"], ["登录邮箱", data.identity.email], ["账号状态", data.identity.status], ["身份创建时间", formatTime(data.identity.createdAt)]]} /></div><div className="privacy-box"><ShieldCheck size={18} /><div><strong>凭据不可见</strong><span>管理端不返回密码、密码哈希、访问令牌或刷新令牌。</span></div></div></>}{tab === 'devices' && <><Problem>{devices.error}</Problem><div className="table-scroll"><table><thead><tr><th>设备</th><th>状态</th><th>最近活跃</th><th>令牌轮换</th></tr></thead><tbody>{devices.data?.items?.map(device => <tr key={device.sessionId}><td><strong>{device.deviceName || device.deviceId}</strong><small className="table-sub mono">{device.clientVersion || '未知版本'}</small></td><td><State value={device.status} /></td><td>{formatTime(device.lastSeenAt)}</td><td>{device.refreshGeneration ?? '—'}</td></tr>)}</tbody></table>{!devices.loading && !devices.data?.items?.length && <None text="该账号没有设备会话" />}</div></>}{data && tab === 'sync' && <div className="detail-section"><h3>云同步元数据</h3><Fields items={[["同步实体", Count(data.sync.entityCount)], ["协议请求", Count(data.sync.requestCount)], ["失败请求", Count(data.sync.failedRequests)], ["冲突数", Count(data.sync.conflicts)], ["逻辑删除", Count(data.sync.tombstones)], ["最后请求", formatTime(data.sync.lastRequestAt)], ["最后同步", formatTime(data.sync.lastSyncedAt)]]} /></div>}{data && tab === 'ai' && <><div className="detail-section"><h3>AI 配额</h3><Fields items={[["配置状态", data.aiQuota.status], ["成本预留上限", data.aiQuota.quotaLimit], ["当前预留", data.aiQuota.reservedCost], ["更新时间", formatTime(data.aiQuota.updatedAt)]]} /></div><div className="detail-section"><h3>真实用量账本</h3><Fields items={[["请求数", Count(data.aiUsage.requestCount)], ["预估成本合计", data.aiUsage.estimatedCost], ["实际成本合计", data.aiUsage.actualCost], ["成本未知记录", Count(data.aiUsage.unknownCost)], ["最近使用", formatTime(data.aiUsage.lastUsedAt)]]} /></div></>}{data && tab === 'management' && <div className="detail-section"><h3>最近管理记录</h3>{data.managementRecords.length ? <div className="timeline">{data.managementRecords.map(row => <div key={row.id}><i className={row.result === 'failed' ? 'red' : ''} /><span><strong>{row.action} · {row.result}</strong><small>{row.reason}</small><small>{row.adminEmail} · {formatTime(row.createdAt)}</small>{row.errorCode && <small className="mono">{row.errorCode}</small>}</span></div>)}</div> : <None text="该账号没有管理操作记录" />}</div>}{detail.loading && <None text="正在读取真实账号详情…" />}</div></aside></div>;
}

export function SyncPage({ token, onUnauthorized }) {
  const [range, setRange] = useState('today');
  const [accountId, setAccountId] = useState('');
  const [status, setStatus] = useState('');
  const overview = useP0Remote(`/sync/overview?range=${range}`, token, onUnauthorized);
  const requests = useP0Remote(`/sync/requests?limit=100&accountId=${encodeURIComponent(accountId)}&status=${encodeURIComponent(status)}`, token, onUnauthorized);
  const summary = overview.data?.summary;
  const refresh = () => { overview.refresh(); requests.refresh(); };
  return <><div className="page-actions p0-toolbar"><RangeControl value={range} onChange={setRange} /><button className="btn secondary" onClick={refresh}><RefreshCw size={14} />刷新</button></div><Problem>{overview.error || requests.error}</Problem><div className="metric-grid four small">{[["协议请求", summary?.requests], ["成功", summary?.succeeded], ["失败", summary?.failed], ["同步冲突", summary?.conflicts]].map(([label, value]) => <div className="metric-card" key={label}><div className="metric-card-top"><span>{label}</span><CloudCog size={17} /></div><div className="metric-value-row"><strong>{Count(value)}</strong></div><div className="metric-note">按服务端返回的 {range} 范围统计</div></div>)}</div><div className="data-freshness"><span><CheckCircle2 size={12} />只记录协议元数据，不记录同步正文</span><span>生成时间 {formatTime(overview.data?.generatedAt)}</span></div><div className="dashboard-main-grid"><Section title="操作分布" subtitle="push / pull / snapshot"><div className="table-scroll"><table><thead><tr><th>操作</th><th>请求</th><th>成功</th><th>失败</th><th>冲突</th></tr></thead><tbody>{overview.data?.operations?.map(row => <tr key={row.operation}><td className="mono">{row.operation}</td><td>{Count(row.requests)}</td><td>{Count(row.succeeded)}</td><td>{Count(row.failed)}</td><td>{Count(row.conflicts)}</td></tr>)}</tbody></table>{!overview.loading && !overview.data?.operations?.length && <None />}</div></Section><Section title="错误分类" subtitle="最多展示 20 类"><div className="table-scroll"><table><thead><tr><th>错误码</th><th>次数</th><th>最近发生</th></tr></thead><tbody>{overview.data?.errors?.map(row => <tr key={row.errorCode}><td className="mono">{row.errorCode}</td><td>{row.count}</td><td>{formatTime(row.lastOccurredAt)}</td></tr>)}</tbody></table>{!overview.loading && !overview.data?.errors?.length && <None text="当前范围没有同步错误" />}</div></Section></div><Section title="同步请求明细" subtitle="用户 ID、游标、耗时和结果；不包含服务器配置或命令内容"><div className="filter-bar"><label className="table-search"><input aria-label="按账号 ID 筛选同步" value={accountId} onChange={event => setAccountId(event.target.value)} placeholder="输入完整账号 ID" /></label><select className="select-control" value={status} onChange={event => setStatus(event.target.value)}><option value="">全部结果</option><option value="succeeded">成功</option><option value="failed">失败</option></select></div><div className="table-scroll"><table><thead><tr><th>时间 / 请求 ID</th><th>账号 / 设备</th><th>操作</th><th>结果</th><th>Revision / 游标</th><th>耗时</th><th>错误</th></tr></thead><tbody>{requests.data?.items?.map(row => <tr key={row.requestId}><td>{formatTime(row.createdAt)}<small className="table-sub mono">{row.requestId}</small></td><td className="mono">{row.accountId || '未认证'}<small className="table-sub mono">{row.deviceId || '—'}</small></td><td>{row.operation} · {row.batchSize} 条</td><td><State value={row.status} /></td><td className="mono">{row.revision ?? '—'} / {row.changeSeq ?? '—'}</td><td>{row.durationMs} ms</td><td className="mono">{row.errorCode || '—'}</td></tr>)}</tbody></table>{!requests.loading && !requests.data?.items?.length && <None />}</div></Section></>;
}

export function QuotaTable({ token, onUnauthorized }) {
  const { data, loading, error } = useP0Remote('/ai/quotas?limit=200', token, onUnauthorized);
  return <Section title="账号配额" subtitle="P0 只读；预留成本与真实用量分开呈现"><Problem>{error}</Problem><div className="table-scroll"><table><thead><tr><th>账号</th><th>配额上限</th><th>当前预留</th><th>账本请求</th><th>预估成本</th><th>未知用量</th><th>更新时间</th></tr></thead><tbody>{data?.items?.map(row => <tr key={row.accountId}><td><strong>{row.email || row.accountId}</strong><small className="table-sub mono">{row.accountId}</small></td><td>{row.quotaLimit}</td><td>{row.reservedCost}</td><td>{Count(row.requestCount)}</td><td>{row.estimatedCost}</td><td>{Count(row.unknownUsage)}</td><td>{formatTime(row.updatedAt)}</td></tr>)}</tbody></table>{!loading && !data?.items?.length && <None text="尚未配置账号级配额" />}</div></Section>;
}

export function SystemPage({ token, onUnauthorized }) {
  const [status, setStatus] = useState('');
  const health = useP0Remote('/system/health', token, onUnauthorized);
  const jobs = useP0Remote(`/system/jobs?limit=100&status=${encodeURIComponent(status)}`, token, onUnauthorized);
  const labels = { api: '管理 API', mysql: 'MySQL', background_worker: '后台 Worker', ai_provider: 'AI Provider' };
  return <><Problem>{health.error || jobs.error}</Problem><Section title="服务状态" subtitle="API 与 MySQL 为实时探测；Worker 与 Provider 只展示配置，不冒充在线探测"><div className="service-grid p0-service-grid">{health.data?.services?.map(service => <div className="p0-service-card" key={service.name}><div className="service-card-top"><span className="service-icon"><Activity size={17} /></span><State value={service.status} /></div><strong>{labels[service.name] || service.name}</strong><span className="mono">{service.reason}</span><small>检查时间 {formatTime(service.checkedAt)}</small></div>)}</div>{!health.loading && !health.data?.services?.length && <None text="真实健康状态不可用" />}</Section><Section title="后台任务" subtitle="只显示编排元数据，不返回任务 payload；P0 不提供重试"><div className="filter-bar"><select className="select-control" value={status} onChange={event => setStatus(event.target.value)}><option value="">全部状态</option><option value="pending">pending</option><option value="leased">leased</option><option value="completed">completed</option><option value="failed">failed</option></select><button className="btn secondary" onClick={() => { health.refresh(); jobs.refresh(); }}><RefreshCw size={14} />刷新</button></div><div className="table-scroll"><table><thead><tr><th>任务 ID / 类型</th><th>状态</th><th>尝试次数</th><th>可执行时间</th><th>租约</th><th>最近错误</th><th>完成时间</th></tr></thead><tbody>{jobs.data?.items?.map(job => <tr key={job.jobId}><td className="mono">{job.jobId}<small className="table-sub">{job.jobType}</small></td><td><State value={job.status} /></td><td>{job.attempts} / {job.maxAttempts}</td><td>{formatTime(job.availableAt)}</td><td>{job.leased ? `租约至 ${formatTime(job.leaseUntil)}` : '无'}</td><td className="mono">{job.lastErrorCode || '—'}</td><td>{formatTime(job.completedAt)}</td></tr>)}</tbody></table>{!jobs.loading && !jobs.data?.items?.length && <None />}</div></Section></>;
}

export function AccessControlPanel({ token, onUnauthorized }) {
  const { data, loading, error } = useP0Remote('/access-control', token, onUnauthorized);
  return <><Problem>{error}</Problem>{data && <><Section title="管理员账号" subtitle="仅超级管理员与审计员可见；P0 不提供账号或角色编辑"><div className="table-scroll"><table><thead><tr><th>管理员</th><th>角色</th><th>状态</th><th>创建时间</th><th>最近登录</th></tr></thead><tbody>{data.administrators.map(admin => <tr key={admin.id}><td><strong>{admin.email}</strong><small className="table-sub mono">{admin.id}</small></td><td className="mono">{admin.role}</td><td><State value={admin.status} /></td><td>{formatTime(admin.createdAt)}</td><td>{formatTime(admin.lastLoginAt)}</td></tr>)}</tbody></table></div></Section><Section title="角色权限边界" subtitle="权限由服务端强制校验，关键操作需要二次确认、operationId 与审计"><div className="role-grid">{data.roles.map(role => <div className="p0-role-card" key={role.name}><ShieldCheck size={18} /><strong>{role.name}</strong><p>{role.description}</p></div>)}</div></Section></>}{loading && <None text="正在读取管理员与角色…" />}</>;
}
