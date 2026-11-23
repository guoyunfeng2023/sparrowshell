import React, { useCallback, useEffect, useState } from 'react';
import {
  Activity, AlertTriangle, ArrowRight, Ban, Bird, Check, CheckCircle2, ChevronRight,
  Circle, CloudCog, CreditCard, Database, FileClock, Laptop, LayoutDashboard, LockKeyhole,
  LogOut, Mail, MessageSquareText, Paperclip, RefreshCw, Rocket, Search, ShieldCheck, Sparkles, UserRoundCog,
  Users, X,
} from 'lucide-react';
import { adminDownload, adminRequest, formatTime, newOperationId } from './api';
import { AccessControlPanel, QuotaTable, RangeControl, SyncPage, SystemPage, UserDetailDrawer } from './AdminP0';

const navigation = [
  ['dashboard', '运营看板', LayoutDashboard, '运营中心'],
  ['users', '用户管理', Users, '运营中心'],
  ['devices', '设备与会话', Laptop, '运营中心'],
  ['sync', '云同步监控', CloudCog, '服务管理'],
  ['ai', 'AI 服务与配额', Sparkles, '服务管理'],
  ['billing', '充值与订阅', CreditCard, '服务管理'],
  ['versions', '客户端版本', Rocket, '服务管理'],
  ['feedback', '问题反馈', MessageSquareText, '服务管理'],
  ['system', '系统运维', Activity, '系统管理'],
  ['audits', '权限与审计', ShieldCheck, '系统管理'],
];
const titles = {
  dashboard: ['运营看板', '只展示服务返回的真实账号与会话数据'], users: ['用户管理', '禁用、恢复与设备撤销均需要 MFA 并写入审计'],
  devices: ['设备与会话', '撤销云端会话不会中断本地 SSH'], sync: ['云同步监控', '只提供协议元数据，不读取同步正文'],
  ai: ['AI 服务与配额', '读取真实用量账本，不展示提示词或模型响应'], versions: ['客户端版本', 'Mac 与 Windows 客户端独立更新发布管理'],
  billing: ['充值与订阅', '查询真实订单、支付与许可证周期，不展示完整激活码或任何凭据'],
  system: ['系统运维', '服务健康与后台任务'], audits: ['权限与审计', '关键操作不可变审计记录'],
  feedback: ['问题反馈', '查看用户主动提交的问题与附件，记录处理进度'],
};
const roleLabels = { SUPER_ADMIN: '超级管理员', SUPPORT_ADMIN: '用户支持专员', VIEWER: '运营查看员', AUDITOR: '安全审计员' };

function Badge({ status }) {
  const labels = { active: '正常', disabled: '已禁用', valid: '有效', revoked: '已撤销', success: '成功', failed: '失败', pending: '待处理', processing: '处理中', resolved: '已解决' };
  const tones = { active: 'green', disabled: 'red', valid: 'green', revoked: 'gray', success: 'green', failed: 'red', pending: 'amber', processing: 'blue', resolved: 'green' };
  return <span className={`status-badge ${tones[status] || 'gray'}`}><i />{labels[status] || status}</span>;
}
function PageTitle({ page, onRefresh, busy }) {
  return <div className="page-title-row"><div><div className="eyebrow">SPARROWSHELL / {page.toUpperCase()}</div><h1>{titles[page][0]}</h1><p>{titles[page][1]}</p></div>{onRefresh && <div className="page-actions"><button className="btn secondary" disabled={busy} onClick={onRefresh}><RefreshCw size={15} className={busy ? 'spin' : ''} />刷新</button></div>}</div>;
}
function Panel({ title, subtitle, children }) {
  return <section className="panel">{(title || subtitle) && <header className="panel-header"><div><h2>{title}</h2>{subtitle && <p>{subtitle}</p>}</div></header>}{children}</section>;
}
function Notice({ error, children }) {
  if (!error && !children) return null;
  return <div className={`inline-notice ${error ? 'warning' : ''}`}>{error ? <AlertTriangle size={16} /> : <ShieldCheck size={16} />}<span>{error || children}</span></div>;
}
function Empty({ title = '暂无数据', note = '这是服务端返回的真实空结果' }) {
  return <div className="empty-state"><Database size={24} /><strong>{title}</strong><span>{note}</span></div>;
}
function UnavailablePage({ page, onRefresh, busy, error }) {
  return <><PageTitle page={page} onRefresh={onRefresh} busy={busy} /><Notice error={error}>该区域尚未接入管理接口，不展示模拟指标或虚构成功状态。</Notice><Panel><Empty title="真实数据暂不可用" note={error || '等待对应服务端能力接入后开放'} /></Panel></>;
}
function Login({ onLogin }) {
  const [email, setEmail] = useState(() => localStorage.getItem('sparrow-admin-email') || '');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const submit = async event => {
    event.preventDefault(); setBusy(true); setError('');
    try {
      const result = await adminRequest('/auth/login', { method: 'POST', body: JSON.stringify({ email, password }) });
      localStorage.setItem('sparrow-admin-email', email.trim()); onLogin(result);
    } catch (reason) { setError(reason.message); }
    finally { setBusy(false); }
  };
  return <div className="auth-page"><aside className="auth-aside"><div className="auth-brand"><span className="brand-mark"><Bird size={25} /></span><div><strong>SparrowShell</strong><small>运营管理中心</small></div></div><div className="auth-aside-content"><span className="auth-kicker">ADMIN CONSOLE</span><h1>让每一次管理操作<br />清晰、可控、可追溯。</h1><p>管理员与用户账号完全隔离，关键操作需要二次确认并保留审计。</p><div className="auth-features"><div><span><ShieldCheck size={19} /></span><div><strong>独立管理员身份</strong><small>账号密码与短时管理会话</small></div></div><div><span><LockKeyhole size={19} /></span><div><strong>令牌仅存于内存</strong><small>刷新页面后需要重新登录</small></div></div><div><span><FileClock size={19} /></span><div><strong>关键操作全程留痕</strong><small>操作原因、结果与错误分类可追溯</small></div></div></div></div></aside><main className="auth-main"><section className="auth-card" aria-label="管理员登录"><div className="auth-heading"><span className="auth-heading-icon"><UserRoundCog size={23} /></span><div><span className="eyebrow">AUTHENTICATED CONSOLE</span><h2>管理员登录</h2><p>使用服务端引导创建的独立管理员账号</p></div></div><form onSubmit={submit}><label className="auth-field"><span>管理员邮箱</span><div className="auth-input"><Mail size={17} /><input aria-label="管理员邮箱" type="email" value={email} onChange={e => setEmail(e.target.value)} autoComplete="username" required autoFocus /></div></label><label className="auth-field"><span>登录密码</span><div className="auth-input"><LockKeyhole size={17} /><input aria-label="登录密码" type="password" value={password} onChange={e => setPassword(e.target.value)} autoComplete="current-password" required /></div></label>{error && <div className="auth-error" role="alert"><AlertTriangle size={15} /><span>{error}</span></div>}<button className="btn primary auth-submit" disabled={busy}>{busy ? <><RefreshCw size={16} className="spin" />正在验证…</> : <>进入管理后台<ArrowRight size={16} /></>}</button></form><footer className="auth-card-footer"><ShieldCheck size={13} />登录仅需账号密码；账号与设备危险操作仍需 MFA</footer></section></main></div>;
}
function Filters({ search, setSearch, status, setStatus, options, placeholder, children }) {
  return <div className="filter-bar"><label className="table-search"><Search size={15} /><input aria-label="搜索记录" value={search} onChange={e => setSearch(e.target.value)} placeholder={placeholder} /></label><select aria-label="状态筛选" className="select-control" value={status} onChange={e => setStatus(e.target.value)}>{options.map(([value, label]) => <option value={value} key={value}>{label}</option>)}</select>{children}</div>;
}
function useRemote(path, token, onUnauthorized, dependencies = []) {
  const [state, setState] = useState({ data: null, loading: Boolean(path), error: '' });
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    if (!path) {
      setState({ data: null, loading: false, error: '' });
      return undefined;
    }
    const controller = new AbortController();
    setState(previous => ({ ...previous, loading: true, error: '' }));
    adminRequest(path, { token, onUnauthorized, signal: controller.signal })
      .then(data => setState({ data, loading: false, error: '' }))
      .catch(error => { if (error.name !== 'AbortError') setState(previous => ({ ...previous, loading: false, error: error.message })); });
    return () => controller.abort();
  }, [path, token, revision, ...dependencies]);
  return { ...state, refresh: () => setRevision(value => value + 1) };
}
function Dashboard({ token, onUnauthorized, navigate }) {
  const [range, setRange] = useState('today');
  const { data, loading, error, refresh } = useRemote(`/dashboard?range=${range}`, token, onUnauthorized);
  const users = data?.users; const devices = data?.devices;
  const metrics = [['当前注册用户', users?.current, Users, 'users', '不含已删除账号'], ['累计注册用户', users?.cumulative, UserRoundCog, 'users', '包含历史账号'], ['范围内新增', users?.new, CheckCircle2, 'users', '按所选时间范围'], ['有效设备会话', devices?.validSessions, Laptop, 'devices', '未撤销、未过期且账号正常']];
  return <><PageTitle page="dashboard" onRefresh={refresh} busy={loading} /><div className="page-actions p0-toolbar"><RangeControl value={range} onChange={setRange} /></div><Notice error={error} /><div className="metric-grid four">{metrics.map(([label, value, Icon, target, note]) => <button className="metric-card" key={label} onClick={() => navigate(target)}><div className="metric-card-top"><span>{label}</span><span className="metric-icon"><Icon size={17} /></span></div><div className="metric-value-row"><strong>{value == null ? '—' : Number(value).toLocaleString('zh-CN')}</strong></div><div className="metric-note">{note}</div></button>)}</div><div className="data-freshness"><span><CheckCircle2 size={12} />口径 {data?.metricVersion || '等待真实数据'} · {range === 'today' ? '北京时间自然日' : `${range} 滚动窗口`}</span><span>生成时间 {formatTime(data?.generatedAt)}</span></div><div className="dashboard-secondary-grid"><Panel title="同步协议" subtitle="不包含同步正文"><div className="ai-summary">{[['请求', data?.sync?.requests], ['失败', data?.sync?.failed], ['冲突', data?.sync?.conflicts]].map(([label, value]) => <div key={label}><span>{label}</span><strong>{value == null ? '—' : Number(value).toLocaleString('zh-CN')}</strong><small>范围内真实协议记录</small></div>)}</div></Panel><Panel title="AI 用量" subtitle="成本未知的记录单独计数"><div className="ai-summary">{[['请求', data?.ai?.requests], ['失败', data?.ai?.failed], ['未知用量', data?.ai?.unknownUsage]].map(([label, value]) => <div key={label}><span>{label}</span><strong>{value == null ? '—' : Number(value).toLocaleString('zh-CN')}</strong><small>真实用量账本</small></div>)}</div></Panel><Panel title="账号与任务状态" subtitle="不按缺失数据推算为 0"><div className="config-list">{(data?.users?.states || []).map(row => <div key={row.status}><span><strong>账号 · {row.status}</strong></span><code>{row.count}</code></div>)}{(data?.jobs || []).map(row => <div key={row.status}><span><strong>任务 · {row.status}</strong></span><code>{row.count}</code></div>)}{(data?.releases || []).map(row => <div key={row.status}><span><strong>版本 · {row.status}</strong></span><code>{row.count}</code></div>)}{data && !data.users.states.length && !data.jobs.length && !data.releases.length && <Empty />}</div></Panel></div><Notice>此处不展示推算增长率、虚构健康状态或 SSH 在线数；统计均来自服务端 SQL 聚合。</Notice></>;
}
function UsersPage({ token, onUnauthorized, onDangerous, canOperate }) {
  const [search, setSearch] = useState(''); const [status, setStatus] = useState(''); const [selected, setSelected] = useState(null);
  const path = `/users?limit=100&query=${encodeURIComponent(search)}&status=${encodeURIComponent(status)}`;
  const { data, loading, error, refresh } = useRemote(path, token, onUnauthorized);
  return <><PageTitle page="users" onRefresh={refresh} busy={loading} /><Panel><Filters search={search} setSearch={setSearch} status={status} setStatus={setStatus} options={[['', '全部状态'], ['active', '正常'], ['disabled', '已禁用']]} placeholder="搜索用户 ID、昵称或邮箱" /><div className="table-meta"><span>共 {data?.total ?? '—'} 个真实账号</span><span><ShieldCheck size={12} />不包含服务器与终端内容</span></div><div className="table-scroll"><table><thead><tr><th>用户</th><th>状态</th><th>注册时间</th><th>最近云端活跃</th><th>有效设备</th><th>Revision</th><th className="align-right">操作</th></tr></thead><tbody>{data?.items?.map(user => <tr key={user.id}><td><button className="user-cell user-detail-link" onClick={() => setSelected(user)}><span className="user-avatar">{(user.nickname || user.email || 'U')[0].toUpperCase()}</span><span><strong>{user.nickname || user.email || '未命名账号'}</strong><small>{user.id} · {user.email || '邮箱未登记'}</small></span></button></td><td><Badge status={user.status} /></td><td className="mono muted">{formatTime(user.createdAt)}</td><td className="mono muted">{formatTime(user.lastActiveAt)}</td><td>{user.deviceCount} 台</td><td className="mono">{user.revision}</td><td className="align-right"><button className="table-action" onClick={() => setSelected(user)}>详情</button>{canOperate && <button className={`table-action ${user.status === 'active' ? 'danger' : ''}`} onClick={() => onDangerous({ type: 'account', target: user })}>{user.status === 'active' ? '禁用' : '恢复'}</button>}</td></tr>)}</tbody></table>{!loading && !data?.items?.length && <Empty />}</div></Panel><Notice error={error} />{selected && <UserDetailDrawer account={selected} token={token} onUnauthorized={onUnauthorized} onClose={() => setSelected(null)} />}</>;
}
function DevicesPage({ token, onUnauthorized, onDangerous, canOperate }) {
  const [search, setSearch] = useState(''); const [status, setStatus] = useState('');
  const path = `/devices?limit=200&query=${encodeURIComponent(search)}&status=${encodeURIComponent(status)}`;
  const { data, loading, error, refresh } = useRemote(path, token, onUnauthorized);
  return <><PageTitle page="devices" onRefresh={refresh} busy={loading} /><Notice>撤销只使云端会话失效，不删除本地数据，也不会中断现有 SSH。</Notice><Panel><Filters search={search} setSearch={setSearch} status={status} setStatus={setStatus} options={[['', '全部会话'], ['valid', '有效'], ['revoked', '已撤销']]} placeholder="搜索设备、用户邮箱或设备 ID" /><div className="table-scroll"><table><thead><tr><th>设备</th><th>所属用户</th><th>客户端</th><th>最近活跃</th><th>会话状态</th><th className="align-right">操作</th></tr></thead><tbody>{data?.items?.map(device => <tr key={device.sessionId}><td><strong>{device.deviceName || '未命名 Mac'}</strong><small className="table-sub mono">{device.deviceId}</small></td><td><strong className="table-primary">{device.accountEmail || device.accountId}</strong></td><td>{device.clientVersion || '—'} / ARM64</td><td className="mono muted">{formatTime(device.lastSeenAt)}</td><td><Badge status={device.status} /></td><td className="align-right">{canOperate && <button className="table-action danger" disabled={device.status !== 'valid'} onClick={() => onDangerous({ type: 'device', target: device })}>撤销会话</button>}</td></tr>)}</tbody></table>{!loading && !data?.items?.length && <Empty />}</div></Panel><Notice error={error} /></>;
}
function AiPage({ token, onUnauthorized }) {
  const { data, loading, error, refresh } = useRemote('/ai/overview?range=today', token, onUnauthorized);
  const ai = data?.ai; const available = ai?.status === 'available';
  const value = number => available ? Number(number).toLocaleString('zh-CN') : '—';
  return <><PageTitle page="ai" onRefresh={refresh} busy={loading} /><Notice error={error || (!available ? '用量账本尚未配置或查询失败，缺失指标不按 0 展示。' : '')} /><div className="metric-grid four small">{[['请求总量', ai?.requests], ['成功请求', ai?.succeeded], ['失败请求', ai?.failed], ['已上报 Token', available ? ai.inputTokens + ai.outputTokens : null]].map(([label, number]) => <div className="metric-card" key={label}><div className="metric-card-top"><span>{label}</span><Sparkles size={17} /></div><div className="metric-value-row"><strong>{value(number)}</strong></div><div className="metric-note">近 24 小时真实账本</div></div>)}</div><Panel title="最近用量" subtitle="只含路由与用量元数据"><div className="table-scroll"><table><thead><tr><th>客户端请求 ID</th><th>路由 / 操作</th><th>Token</th><th>结果</th><th>时间</th></tr></thead><tbody>{available && ai.recent.map(row => <tr key={row.clientRequestId}><td className="mono">{row.clientRequestId}</td><td>{row.routeId} / {row.operation}</td><td>{row.usageStatus === 'reported' ? `${row.inputTokens} / ${row.outputTokens}` : '未知'}</td><td>{row.status}</td><td>{formatTime(row.createdAt)}</td></tr>)}</tbody></table>{(!available || !ai.recent.length) && <Empty title={available ? '当前范围没有账本记录' : '真实账本暂不可用'} note={available ? '这是真实空结果' : '请检查服务端持久化配置'} />}</div></Panel><QuotaTable token={token} onUnauthorized={onUnauthorized} /></>;
}
const billingPlanLabels = { personal: '个人版', high: '高频版' };
function BillingBadge({ status }) {
  const labels = { pending: '待支付', paid: '已支付', expired: '已过期', bound: '已绑定', unbound: '未绑定', active: '生效中', inactive: '无周期', scheduled: '待生效', expiring: '即将到期', exhausted: '已用完', activated: '已激活', 'not-activated': '未激活', unknown: '未知' };
  const tones = { pending: 'amber', paid: 'green', expired: 'gray', bound: 'green', unbound: 'gray', active: 'green', inactive: 'gray', scheduled: 'blue', expiring: 'amber', exhausted: 'red', activated: 'green', 'not-activated': 'gray', unknown: 'gray' };
  return <span className={`status-badge ${tones[status] || 'gray'}`}><i />{labels[status] || status}</span>;
}
function BillingPage({ token, onUnauthorized }) {
  const [tab, setTab] = useState('orders');
  const [search, setSearch] = useState('');
  const [status, setStatus] = useState('');
  const [plan, setPlan] = useState('');
  const [page, setPage] = useState(0);
  const subscriptions = tab === 'subscriptions';
  const limit = subscriptions ? 20 : 100;
  const path = subscriptions
    ? `/billing/subscriptions?limit=${limit}&offset=${page * limit}&query=${encodeURIComponent(search)}&status=${encodeURIComponent(status)}&plan=${encodeURIComponent(plan)}`
    : `/billing/${tab}?limit=100&query=${encodeURIComponent(search)}&status=${encodeURIComponent(status)}`;
  const list = useRemote(path, token, onUnauthorized);
  const summary = useRemote(subscriptions ? '/billing/subscriptions/summary' : null, token, onUnauthorized);
  const { data, loading, error } = list;
  const refresh = () => { list.refresh(); if (subscriptions) summary.refresh(); };
  const changeTab = value => { setTab(value); setSearch(''); setStatus(''); setPlan(''); setPage(0); };
  const changeSearch = value => { setSearch(value); setPage(0); };
  const changeStatus = value => { setStatus(value); setPage(0); };
  const changePlan = value => { setPlan(value); setPage(0); };
  const options = tab === 'orders'
    ? [['', '全部订单状态'], ['pending', '待支付'], ['paid', '已支付'], ['expired', '已过期']]
    : subscriptions
      ? [['', '全部周期状态'], ['active', '生效中'], ['scheduled', '待生效'], ['expiring', '即将到期'], ['exhausted', '已用完'], ['expired', '已过期']]
      : [['', '全部绑定状态'], ['bound', '已绑定'], ['unbound', '未绑定']];
  const placeholder = tab === 'orders' ? '搜索订单号、支付宝流水号或许可证 ID' : subscriptions ? '搜索周期 ID、许可证 ID 或设备 ID' : '搜索许可证 ID、绑定设备 ID 或完整激活码';
  const totalPages = Math.max(1, Math.ceil((data?.total || 0) / limit));
  const count = value => value == null ? '—' : Number(value).toLocaleString('zh-CN');
  const metrics = [
    ['生效中', summary.data?.active, '当前可使用的订阅周期', ''],
    ['待生效', summary.data?.scheduled, '已购买但尚未开始', 'blue'],
    ['7 天内到期', summary.data?.expiring, '需要关注续费的周期', 'amber'],
    ['次数耗尽', summary.data?.exhausted, '额度已使用完毕', 'red'],
  ];
  return <>
    <PageTitle page="billing" onRefresh={refresh} busy={loading || (subscriptions && summary.loading)} />
    <div className="page-actions p0-toolbar"><div className="segmented" aria-label="充值查询类型">{[['orders', '充值订单'], ['licenses', '许可证'], ['subscriptions', '订阅周期']].map(([id, label]) => <button key={id} className={tab === id ? 'active' : ''} onClick={() => changeTab(id)}>{label}</button>)}</div></div>
    {subscriptions && <>
      <div className="metric-grid four small">{metrics.map(([label, value, note, tone]) => <div className={`metric-card ${tone}`} key={label}><div className="metric-card-top"><span>{label}</span><span className="metric-icon"><CreditCard size={17} /></span></div><div className="metric-value-row"><strong>{count(value)}</strong></div><div className="metric-note">{note}</div></div>)}</div>
      <div className="data-freshness"><span><CheckCircle2 size={12} />共 {count(summary.data?.total)} 个周期 · 已到期 {count(summary.data?.expired)}</span><span>个人版生效 {count(summary.data?.personalActive)} · 高频版生效 {count(summary.data?.highActive)}</span></div>
    </>}
    <Notice>{subscriptions ? '订阅均为手动续费，不自动扣款；这里只读展示周期、额度和绑定状态。' : '只读查询，不支持退款、补单或解绑；不返回订单访问令牌、支付二维码、完整激活码、密钥密文或设备令牌。'}</Notice>
    <Panel>
      <Filters search={search} setSearch={changeSearch} status={status} setStatus={changeStatus} options={options} placeholder={placeholder}>{subscriptions && <select aria-label="套餐筛选" className="select-control" value={plan} onChange={event => changePlan(event.target.value)}><option value="">全部套餐</option><option value="personal">个人版</option><option value="high">高频版</option></select>}</Filters>
      <div className="table-meta"><span>共 {data?.total ?? '—'} 条真实记录{!subscriptions && data?.total > 100 ? '，当前显示最近 100 条' : ''}</span><span><ShieldCheck size={12} />{subscriptions ? '不展示完整激活码或设备令牌' : '支付与许可证敏感凭据不可见'}</span></div>
      {tab === 'orders' ? <div className="table-scroll"><table><thead><tr><th>订单 / 支付流水</th><th>套餐</th><th>金额</th><th>状态</th><th>许可证 ID / 绑定状态</th><th>创建时间</th><th>支付时间</th><th>订单过期时间</th></tr></thead><tbody>{data?.items?.map(order => <tr key={order.id}><td><strong className="mono">{order.id}</strong><small className="table-sub mono">{order.alipayTradeNo || '暂无支付流水'}</small></td><td>{billingPlanLabels[order.plan] || order.plan}</td><td>¥ {Number(order.amount).toFixed(2)}</td><td><BillingBadge status={order.status} /></td><td><strong className="mono">{order.licenseId || '支付后生成'}</strong>{order.licenseId && <small className="table-sub"><BillingBadge status={order.licenseBound == null ? 'unknown' : order.licenseBound ? 'bound' : 'unbound'} /></small>}</td><td className="mono muted">{formatTime(order.createdAt)}</td><td className="mono muted">{formatTime(order.paidAt)}</td><td className="mono muted">{formatTime(order.expiresAt)}</td></tr>)}</tbody></table>{!loading && !data?.items?.length && <Empty title="暂无充值订单" note="当前筛选条件没有真实订单记录" />}</div> : subscriptions ? <>
        <div className="table-scroll"><table><thead><tr><th>周期 / 许可证 ID</th><th>绑定设备</th><th>套餐</th><th>周期状态</th><th>已用 / 剩余</th><th>开始时间</th><th>到期时间</th></tr></thead><tbody>{data?.items?.map(period => <tr key={period.id}><td><strong className="mono">{period.id}</strong><small className="table-sub mono">{period.licenseId}</small></td><td><BillingBadge status={period.bound ? 'bound' : 'unbound'} /><small className="table-sub mono">{period.boundDeviceId || '尚未绑定设备'}</small></td><td>{billingPlanLabels[period.plan] || period.plan}<small className="table-sub">共 {period.quota} 次</small></td><td><BillingBadge status={period.status} /><small className="table-sub">{period.autoRenew ? '自动续费' : '手动续费，不自动扣款'}</small></td><td className="mono">{period.used} / {period.remaining} 次</td><td className="mono muted">{formatTime(period.periodStart)}</td><td className="mono muted">{formatTime(period.periodEnd)}</td></tr>)}</tbody></table>{!loading && !data?.items?.length && <Empty title="暂无订阅周期" note="当前筛选条件没有真实订阅周期记录" />}</div>
        <div className="pagination"><span>第 {page + 1} / {totalPages} 页 · 每页 20 条</span><div><button disabled={loading || page === 0} onClick={() => setPage(value => value - 1)}>上一页</button><button disabled={loading || page + 1 >= totalPages} onClick={() => setPage(value => value + 1)}>下一页</button></div></div>
      </> : <div className="table-scroll"><table><thead><tr><th>许可证 ID</th><th>激活状态 / 时间</th><th>绑定状态 / 设备</th><th>当前或最近套餐</th><th>周期状态</th><th>当前额度</th><th>已用 / 剩余</th><th>当前或最近到期时间</th><th>更新时间</th></tr></thead><tbody>{data?.items?.map(license => <tr key={license.id}><td className="mono">{license.id}</td><td><BillingBadge status={license.activated == null ? 'unknown' : license.activated ? 'activated' : 'not-activated'} /><small className="table-sub mono">{formatTime(license.activatedAt)}</small></td><td><BillingBadge status={license.bound ? 'bound' : 'unbound'} /><small className="table-sub mono">{license.boundDeviceId || '尚未绑定设备'}</small></td><td>{billingPlanLabels[license.plan || license.latestPlan] || '—'}</td><td><BillingBadge status={license.periodEnd ? 'active' : license.latestPeriodEnd ? 'expired' : 'inactive'} /></td><td>{license.quota == null ? '—' : `${license.quota} 次`}</td><td>{license.used == null ? '—' : `${license.used} / ${license.remaining} 次`}</td><td className="mono muted">{formatTime(license.periodEnd || license.latestPeriodEnd)}</td><td className="mono muted">{formatTime(license.updatedAt)}</td></tr>)}</tbody></table>{!loading && !data?.items?.length && <Empty title="暂无可查询许可证" note="当前筛选条件没有真实许可证记录" />}</div>}
    </Panel>
    <Notice error={error || (subscriptions ? summary.error : '')} />
  </>;
}
const releasePlatforms = [
  ['darwin-aarch64', 'Mac 客户端', 'Mac 客户端（Apple Silicon）', '.dmg'],
  ['windows-x86_64', 'Windows 客户端', 'Windows 客户端（x64）', '.exe'],
];
function ReleasesPage({ token, onUnauthorized, onDangerous, onCreate, canOperate }) {
  const [target, setTarget] = useState('darwin-aarch64');
  const { data, loading, error, refresh } = useRemote('/releases', token, onUnauthorized);
  const policyLabels = { force: '强制更新', optional: '可选择更新', silent: '不提示更新' };
  const platform = releasePlatforms.find(([value]) => value === target);
  const releases = data?.items?.filter(release => release.target === target) || [];
  const actions = release => {
    if (release.state === 'draft' || release.state === 'canary') return [['stable', '发布'], ['withdraw', '撤回']];
    if (release.state === 'paused') return [['resume', '恢复'], ['withdraw', '撤回']];
    if (release.state === 'stable') return [['pause', '暂停'], ['withdraw', '撤回']];
    return [];
  };
  return <>
    <PageTitle page="versions" onRefresh={refresh} busy={loading} />
    <Notice>Mac 与 Windows 各平台独立发布，仅向所选平台的适用客户端提供更新；发布操作需填写原因并二次确认，所有变更写入审计。</Notice>
    <div className="page-actions p0-toolbar">
      <div className="segmented" aria-label="客户端平台">{releasePlatforms.map(([value, label]) => <button key={value} className={target === value ? 'active' : ''} onClick={() => setTarget(value)}>{label}</button>)}</div>
      {canOperate && <button className="btn primary" onClick={() => onCreate(target)}>新建{platform[1]}版本草稿</button>}
    </div>
    <Panel title={`${platform[1]}版本记录`} subtitle={`更新目标：${target}`}>
      <div className="table-scroll"><table><thead><tr><th>版本</th><th>状态</th><th>更新策略</th><th>发布时间</th><th className="align-right">操作</th></tr></thead><tbody>{releases.map(release => <tr key={release.id}><td><strong>{release.version}</strong><small className="table-sub mono">{release.id}</small></td><td><Badge status={release.state} /></td><td>{policyLabels[release.updatePolicy] || '可选择更新'}</td><td className="mono muted">{formatTime(release.publishedAt)}</td><td className="align-right">{canOperate && actions(release).map(([operation, label]) => <button className={`table-action ${operation === 'withdraw' ? 'danger' : ''}`} key={operation} onClick={() => onDangerous({ type: 'release', target: release, operation })}>{label}</button>)}</td></tr>)}</tbody></table>{!loading && !releases.length && <Empty title={`${platform[1]}暂无版本`} note="当前平台还没有真实版本记录" />}</div>
    </Panel>
    <Notice error={error} />
  </>;
}
function ReleaseDraftModal({ initialTarget = 'darwin-aarch64', onClose, onConfirm, busy, error }) {
  const [release, setRelease] = useState({ target: initialTarget, version: '', artifactUrl: '', updatePolicy: 'optional', releaseNotes: '' });
  const update = (key, value) => setRelease(previous => ({ ...previous, [key]: value }));
  const platform = releasePlatforms.find(([value]) => value === release.target) || releasePlatforms[0];
  const valid = /^\d+\.\d+\.\d+$/.test(release.version.trim()) && /^https:\/\/[^\s]+$/i.test(release.artifactUrl.trim());
  return <div className="modal-backdrop"><section className="modal" role="dialog" aria-modal="true" aria-label="新建版本草稿"><header><div><h2>新建版本草稿</h2><p>创建草稿后，确认发布即可向{platform[1]}提供更新</p></div><button className="icon-btn" aria-label="关闭" onClick={onClose}><X size={18} /></button></header><div className="modal-body"><label className="field"><span>客户端平台 *</span><select value={release.target} onChange={event => update('target', event.target.value)}>{releasePlatforms.map(([value, , label]) => <option value={value} key={value}>{label}</option>)}</select></label><label className="field"><span>版本号 *</span><input value={release.version} onChange={event => update('version', event.target.value)} placeholder="例如 1.1.0" autoFocus /></label><label className="field"><span>制品 URL *</span><input type="url" value={release.artifactUrl} onChange={event => update('artifactUrl', event.target.value)} placeholder={`https://example.com/SparrowShell-1.1.0${platform[3]}`} /></label><label className="field"><span>更新策略 *</span><select value={release.updatePolicy} onChange={event => update('updatePolicy', event.target.value)}><option value="force">强制更新</option><option value="optional">可选择更新</option><option value="silent">不提示更新</option></select></label><label className="field"><span>更新内容</span><textarea rows="5" maxLength={10000} value={release.releaseNotes} onChange={event => update('releaseNotes', event.target.value)} placeholder={'例如：\n1. 新增服务器批量执行\n2. 优化 AI 响应速度\n3. 修复已知问题'} /></label>{error && <div className="auth-error"><AlertTriangle size={15} />{error}</div>}</div><footer className="modal-footer"><button className="btn secondary" onClick={onClose}>取消</button><button className="btn primary" disabled={!valid || busy} onClick={() => onConfirm({ release: { target: release.target, version: release.version.trim(), artifactUrl: release.artifactUrl.trim(), updatePolicy: release.updatePolicy, releaseNotes: release.releaseNotes.trim() } })}>{busy ? '正在创建…' : '创建草稿'}</button></footer></section></div>;
}
function AuditsPage({ token, onUnauthorized }) {
  const { data, loading, error, refresh } = useRemote('/audits?limit=200', token, onUnauthorized);
  return <><PageTitle page="audits" onRefresh={refresh} busy={loading} /><Notice>管理员角色由服务端强制校验。新增管理员和角色编辑器尚未开放，不提供虚假操作入口。</Notice><Panel title="操作审计" subtitle="成功与业务失败均保留记录"><div className="table-scroll"><table><thead><tr><th>时间</th><th>管理员</th><th>操作</th><th>目标</th><th>原因</th><th>结果</th><th>错误分类</th></tr></thead><tbody>{data?.items?.map(audit => <tr key={audit.id}><td className="mono muted">{formatTime(audit.createdAt)}</td><td>{audit.adminEmail}</td><td className="mono">{audit.action}</td><td className="mono">{audit.targetType} / {audit.targetId}</td><td>{audit.reason}</td><td><Badge status={audit.result} /></td><td className="mono">{audit.errorCode || '—'}</td></tr>)}</tbody></table>{!loading && !data?.items?.length && <Empty />}</div></Panel><Notice error={error} /><AccessControlPanel token={token} onUnauthorized={onUnauthorized} /></>;
}
function FeedbackDetail({ id, token, onUnauthorized, onClose, onUpdated, canOperate }) {
  const { data, loading, error, refresh } = useRemote(`/feedback/${encodeURIComponent(id)}`, token, onUnauthorized);
  const [busy, setBusy] = useState(false); const [actionError, setActionError] = useState('');
  const updateStatus = async status => {
    setBusy(true); setActionError('');
    try {
      await adminRequest(`/feedback/${encodeURIComponent(id)}/status`, { token, onUnauthorized, method: 'PATCH', body: JSON.stringify({ status }) });
      refresh(); onUpdated();
    } catch (reason) { setActionError(reason.message); }
    finally { setBusy(false); }
  };
  const download = async () => {
    setBusy(true); setActionError('');
    try {
      const { blob, filename } = await adminDownload(`/feedback/${encodeURIComponent(id)}/attachment`, { token, onUnauthorized });
      const url = URL.createObjectURL(blob); const anchor = document.createElement('a');
      anchor.href = url; anchor.download = filename; anchor.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (reason) { setActionError(reason.message); }
    finally { setBusy(false); }
  };
  return <div className="drawer-layer"><button className="drawer-dismiss" aria-label="关闭反馈详情" onClick={onClose} /><aside className="drawer feedback-drawer" role="dialog" aria-modal="true" aria-label="反馈详情"><header className="drawer-header"><div><div className="eyebrow">FEEDBACK DETAIL</div><h2>反馈详情</h2></div><button className="icon-btn" onClick={onClose} aria-label="关闭"><X size={18} /></button></header><div className="drawer-content feedback-detail">{loading && <Empty title="正在读取反馈" note="请稍候" />}{error && <Notice error={error} />}{data && <>
    <div className="feedback-detail-top"><Badge status={data.status} /><span className="mono muted">{formatTime(data.createdAt)}</span></div>
    <dl className="feedback-meta"><div><dt>提交人</dt><dd>{data.userId ? <><strong>{data.userNickname || data.userEmail || '已登录用户'}</strong><small>{data.userEmail || data.userId}</small></> : '匿名用户'}</dd></div><div><dt>联系方式</dt><dd>{data.contact || '未填写'}</dd></div><div><dt>反馈编号</dt><dd className="mono">{data.id}</dd></div><div><dt>最近更新</dt><dd>{formatTime(data.updatedAt)}</dd></div></dl>
    <section className="feedback-description-full"><h3>问题描述</h3><p>{data.description}</p></section>
    <section className="feedback-attachment-detail"><h3>截图或附件</h3>{data.attachment ? <button className="btn secondary" disabled={busy} onClick={download}><Paperclip size={15} />{data.attachment.originalName}<small>{Math.ceil(data.attachment.sizeBytes / 1024)} KB</small></button> : <span className="muted">未提交附件</span>}</section>
    {actionError && <Notice error={actionError} />}
  </>}</div>{data && canOperate && <footer className="drawer-footer">{data.status !== 'pending' && <button className="btn secondary" disabled={busy} onClick={() => updateStatus('pending')}>恢复待处理</button>}{data.status !== 'processing' && <button className="btn secondary" disabled={busy} onClick={() => updateStatus('processing')}>标记处理中</button>}{data.status !== 'resolved' && <button className="btn primary" disabled={busy} onClick={() => updateStatus('resolved')}>标记已解决</button>}</footer>}</aside></div>;
}
function FeedbackPage({ token, onUnauthorized, canOperate }) {
  const [status, setStatus] = useState(''); const [selected, setSelected] = useState(null);
  const { data, loading, error, refresh } = useRemote(`/feedback?limit=100&status=${encodeURIComponent(status)}`, token, onUnauthorized);
  return <><PageTitle page="feedback" onRefresh={refresh} busy={loading} /><Notice>这里只展示用户主动提交的反馈内容，不收集或展示终端、服务器配置和 AI 会话。</Notice><Panel><div className="filter-bar"><select className="select-control" aria-label="反馈状态" value={status} onChange={event => setStatus(event.target.value)}><option value="">全部状态</option><option value="pending">待处理</option><option value="processing">处理中</option><option value="resolved">已解决</option></select></div><div className="table-meta"><span>共 {data?.total ?? '—'} 条真实反馈</span><span><ShieldCheck size={12} />附件下载需要管理员鉴权</span></div><div className="table-scroll"><table><thead><tr><th>提交人</th><th>问题描述</th><th>联系方式</th><th>附件</th><th>状态</th><th>提交时间</th><th className="align-right">操作</th></tr></thead><tbody>{data?.items?.map(item => <tr key={item.id}><td><strong>{item.userId ? item.userNickname || item.userEmail || '已登录用户' : '匿名用户'}</strong><small className="table-sub">{item.userEmail || item.userId || '未关联账号'}</small></td><td><button className="feedback-description" title={item.description} onClick={() => setSelected(item)}>{item.description}</button></td><td>{item.contact || '—'}</td><td>{item.attachment ? <span title={item.attachment.originalName}><Paperclip size={13} /> 1</span> : '—'}</td><td><Badge status={item.status} /></td><td className="mono muted">{formatTime(item.createdAt)}</td><td className="align-right"><button className="table-action" onClick={() => setSelected(item)}>详情</button></td></tr>)}</tbody></table>{!loading && !data?.items?.length && <Empty title="暂无反馈" note="当前筛选范围没有用户提交记录" />}</div></Panel><Notice error={error} />{selected && <FeedbackDetail id={selected.id} token={token} onUnauthorized={onUnauthorized} canOperate={canOperate} onClose={() => setSelected(null)} onUpdated={refresh} />}</>;
}
function DangerousModal({ action, onClose, onConfirm, busy, error }) {
  const [reason, setReason] = useState(''); const [mfaCode, setMfa] = useState('');
  const account = action.type === 'account'; const release = action.type === 'release'; const target = action.target; const enabling = account && target.status !== 'active';
  const requiresMfa = !release;
  const operationLabels = { stable: '全量发布', pause: '暂停发布', resume: '恢复发布', withdraw: '撤回版本' };
  const title = account ? `${enabling ? '恢复' : '禁用'}用户账号` : release ? operationLabels[action.operation] : '撤销设备云端会话';
  const impact = account && !enabling ? '将撤销该账号全部云端设备会话，不影响本地数据与活动 SSH。' : release && action.operation === 'stable' ? '发布后，全部适用客户端均可检查到此版本；不影响现有 SSH，制品不可覆盖。' : release ? '只改变更新推荐状态，不影响现有 SSH；制品不可覆盖。' : '只改变云端访问状态，不操作用户服务器。';
  return <div className="modal-backdrop"><section className="modal" role="dialog" aria-modal="true"><header><div><h2>{title}</h2><p>{target.email || target.accountEmail || target.deviceName || target.version} · {target.id || target.sessionId}</p></div><button className="icon-btn" aria-label="关闭" onClick={onClose}><X size={18} /></button></header><div className="modal-body"><div className="impact-box">{impact}</div><label className="field"><span>操作原因 *</span><textarea rows="3" value={reason} onChange={e => setReason(e.target.value)} placeholder="至少 4 个字，将写入审计" autoFocus /></label>{requiresMfa && <label className="field"><span>再次输入 MFA 验证码 *</span><input value={mfaCode} onChange={e => setMfa(e.target.value.replace(/\D/g, '').slice(0, 6))} inputMode="numeric" autoComplete="one-time-code" placeholder="6 位动态验证码" /></label>}{error && <div className="auth-error"><AlertTriangle size={15} />{error}</div>}</div><footer className="modal-footer"><button className="btn secondary" onClick={onClose}>取消</button><button className={`btn ${!enabling ? 'danger' : 'primary'}`} disabled={busy || reason.trim().length < 4 || (requiresMfa && mfaCode.length !== 6)} onClick={() => onConfirm({ reason: reason.trim(), mfaCode })}>{busy ? '正在执行…' : release && action.operation === 'stable' ? '确认发布' : '确认操作'}</button></footer></section></div>;
}
function App() {
  const [session, setSession] = useState(null); const [page, setPage] = useState('dashboard');
  const [action, setAction] = useState(null); const [busy, setBusy] = useState(false); const [actionError, setActionError] = useState('');
  const [toast, setToast] = useState(''); const [refreshKey, setRefreshKey] = useState(0);
  const unauthorized = useCallback(() => { setSession(null); setAction(null); }, []);
  const token = session?.token; const canOperate = ['SUPER_ADMIN', 'SUPPORT_ADMIN'].includes(session?.admin?.role);
  const logout = async () => { try { await adminRequest('/auth/logout', { token, method: 'POST' }); } finally { unauthorized(); } };
  const dangerous = config => { setAction(config); setActionError(''); };
  const confirm = async ({ reason, mfaCode, release }) => {
    setBusy(true); setActionError('');
    const account = action.type === 'account'; const target = action.target;
    const operation = { reason, mfaCode, operationId: newOperationId() };
    let path; let method = 'POST'; let body = operation;
    if (account) {
      path = `/users/${encodeURIComponent(target.id)}/status`; method = 'PATCH';
      body = { ...operation, status: target.status === 'active' ? 'disabled' : 'active', expectedRevision: target.revision };
    } else if (action.type === 'device') path = `/devices/${encodeURIComponent(target.sessionId)}/revoke`;
    else if (action.type === 'release') { path = `/releases/${encodeURIComponent(target.id)}/${action.operation}`; body = { operation }; }
    else { path = '/releases'; body = release; }
    try {
      await adminRequest(path, { token, onUnauthorized: unauthorized, method, body: JSON.stringify(body) });
      setAction(null); setToast(account ? '账号状态已更新' : action.type === 'device' ? '设备会话已撤销' : '版本信息已更新'); setRefreshKey(value => value + 1);
      window.setTimeout(() => setToast(''), 3000);
    } catch (error) { setActionError(error.message); }
    finally { setBusy(false); }
  };
  if (!session) return <Login onLogin={setSession} />;
  const pageProps = { token, onUnauthorized: unauthorized };
  const pageKey = `${page}-${refreshKey}`;
  return <div className="admin-app"><aside className="admin-sidebar"><div className="sidebar-brand"><span className="brand-mark"><Bird size={23} /></span><div><strong>SparrowShell</strong><small>运营管理中心</small></div></div><div className="environment-card"><span className="pulse-dot" /><div><strong>真实管理接口</strong><small>关键操作二次确认并留痕</small></div></div><nav className="admin-nav">{['运营中心', '服务管理', '系统管理'].map(group => <div className="nav-group" key={group}><span className="nav-group-label">{group}</span>{navigation.filter(item => item[3] === group).map(([id, label, Icon]) => <button key={id} className={page === id ? 'active' : ''} onClick={() => setPage(id)}><Icon size={18} /><span>{label}</span></button>)}</div>)}</nav><div className="sidebar-bottom"><div className="privacy-seal"><ShieldCheck size={17} /><div><strong>最小可见原则</strong><span>服务器与终端内容不可见</span></div></div></div></aside><div className="admin-main"><header className="admin-topbar"><div /><div className="topbar-right"><span className="prototype-tag">真实接口 · 非演示数据</span><button className="admin-profile" onClick={logout} title="退出并撤销管理会话"><span>{session.admin.email[0].toUpperCase()}</span><div><strong>{session.admin.email}</strong><small>{roleLabels[session.admin.role] || session.admin.role} · 点击退出</small></div><LogOut size={14} /></button></div></header><main className="admin-content" key={pageKey}>{page === 'dashboard' && <Dashboard {...pageProps} navigate={setPage} />}{page === 'users' && <UsersPage {...pageProps} canOperate={canOperate} onDangerous={dangerous} />}{page === 'devices' && <DevicesPage {...pageProps} canOperate={canOperate} onDangerous={dangerous} />}{page === 'ai' && <AiPage {...pageProps} />}{page === 'billing' && <BillingPage {...pageProps} />}{page === 'versions' && <ReleasesPage {...pageProps} canOperate={['SUPER_ADMIN', 'RELEASE_ADMIN'].includes(session.admin.role)} onDangerous={dangerous} onCreate={target => dangerous({ type: 'releaseCreate', target })} />}{page === 'feedback' && <FeedbackPage {...pageProps} canOperate={canOperate} />}{page === 'audits' && <AuditsPage {...pageProps} />}{page === 'sync' && <><PageTitle page="sync" /><SyncPage {...pageProps} /></>}{page === 'system' && <><PageTitle page="system" /><SystemPage {...pageProps} /></>}</main></div>{action?.type === 'releaseCreate' ? <ReleaseDraftModal initialTarget={action.target} onClose={() => !busy && setAction(null)} onConfirm={confirm} busy={busy} error={actionError} /> : action && <DangerousModal action={action} onClose={() => !busy && setAction(null)} onConfirm={confirm} busy={busy} error={actionError} />}{toast && <div className="toast"><CheckCircle2 size={17} />{toast}<button onClick={() => setToast('')}><X size={14} /></button></div>}</div>;
}
export default App;
