import { useEffect, useState } from 'react';
import { AlertTriangle, Check, Copy, CreditCard, ExternalLink, Eye, EyeOff, KeyRound, Laptop, LoaderCircle, ShieldCheck, X } from 'lucide-react';
import { desktop } from '../../ipc/client';
import type { AiBillingOrder, AiEntitlement } from '../../types';

const copy = {
  zh: {
    title: 'AI 套餐与许可证', close: '关闭', currentPlan: '当前套餐', free: '免费版', personal: '个人版', high: '高频版',
    remaining: '本周期剩余', times: '次', used: '已使用', validUntil: '有效期至', monthlyReset: '每自然月重置 50 次',
    bound: '已绑定本机', unbound: '未绑定设备', independent: '许可证与登录账号独立，可随时解绑并换机激活。',
    choose: '选择套餐', personalDetail: '每个付费周期 200 次', highDetail: '每个付费周期 1000 次', month: '月',
    buy: '支付宝扫码购买', renew: '续费', renewNow: '立即续费', upgradeNow: '立即升级', expired: '套餐已到期', expiredAt: '原套餐到期时间',
    upgradeCarry: '支付成功后，个人版尚未使用的次数将并入高频版，并从支付成功时重新计算一个月。', downgradeBlocked: '高频版当前周期结束前不能降级为个人版。',
    paymentUnavailable: '支付服务暂未开放，仍可使用已有许可证激活。',
    mock: '开发环境模拟支付：不会发生真实扣款。', orderCreated: '支付页已在系统浏览器打开。支付完成后，复制页面中的许可证并回到这里激活。',
    reopen: '重新打开支付页', opening: '正在打开…', order: '当前订单', pending: '待支付', paid: '已支付',
    activateTitle: '激活或换机恢复', licensePlaceholder: '输入支付后获得的许可证', activate: '激活到本机', activating: '正在激活…',
    activated: '许可证已激活，付费额度已生效。', licenseHelp: '换机前请先在旧电脑解绑；许可证只保存在本机客户端，不会同步到云端。',
    currentLicense: '当前激活码', maskedLicense: 'SS-••••-••••-••••-••••', showLicense: '显示激活码', hideLicense: '隐藏', copyLicense: '复制',
    licenseCopied: '激活码已复制，请妥善保存。', noLicense: '本机未保存激活码。',
    deactivate: '解绑本机', deactivateTitle: '确认解绑这台电脑？', deactivateHelp: '解绑后本机恢复免费额度，并删除本机保存的激活码。请先复制并妥善保存，换机时需要使用。',
    cancel: '取消', confirmDeactivate: '确认解绑', deactivating: '正在解绑…', deactivated: '本机已解绑，可前往其他电脑激活。',
    requestFailed: '操作未完成，请稍后重试。', security: '一次 AI 任务只扣一次；失败、超时或手动停止不扣次数。',
  },
  en: {
    title: 'AI plans & license', close: 'Close', currentPlan: 'Current plan', free: 'Free', personal: 'Personal', high: 'High usage',
    remaining: 'Remaining this period', times: 'tasks', used: 'Used', validUntil: 'Valid until', monthlyReset: 'Resets to 50 tasks each calendar month',
    bound: 'Bound to this PC', unbound: 'No device bound', independent: 'Your license is independent of your account. Unbind and activate it on another PC at any time.',
    choose: 'Choose a plan', personalDetail: '200 tasks per paid period', highDetail: '1,000 tasks per paid period', month: 'month',
    buy: 'Buy with Alipay QR', renew: 'Renew', renewNow: 'Renew now', upgradeNow: 'Upgrade now', expired: 'Plan expired', expiredAt: 'Previous plan expired',
    upgradeCarry: 'After payment, all unused Personal tasks carry over to High usage, and a new one-month period starts immediately.', downgradeBlocked: 'You cannot downgrade to Personal before the current High usage period ends.',
    paymentUnavailable: 'Payments are not available yet. You can still activate an existing license.',
    mock: 'Development mock payment: no real charge will occur.', orderCreated: 'Checkout opened in your browser. After payment, copy the license and return here to activate it.',
    reopen: 'Reopen checkout', opening: 'Opening…', order: 'Current order', pending: 'Pending', paid: 'Paid',
    activateTitle: 'Activate or restore on a new PC', licensePlaceholder: 'Enter the license received after payment', activate: 'Activate on this PC', activating: 'Activating…',
    activated: 'License activated. Your paid quota is now available.', licenseHelp: 'Unbind the old PC first. Your license is stored only in the app on this PC and is never synced.',
    currentLicense: 'Current license', maskedLicense: 'SS-••••-••••-••••-••••', showLicense: 'Show license', hideLicense: 'Hide', copyLicense: 'Copy',
    licenseCopied: 'License copied. Keep it somewhere safe.', noLicense: 'No license is stored on this PC.',
    deactivate: 'Unbind this PC', deactivateTitle: 'Unbind this PC?', deactivateHelp: 'This PC will return to the free quota and delete the saved license. Copy and keep it first; you will need it to activate another PC.',
    cancel: 'Cancel', confirmDeactivate: 'Unbind', deactivating: 'Unbinding…', deactivated: 'This PC is unbound. You can now activate the license elsewhere.',
    requestFailed: 'The operation could not be completed. Please try again.', security: 'One AI task costs one credit. Failures, timeouts, and manual stops cost nothing.',
  },
} as const;

interface AiBillingDialogProps {
  locale: 'zh' | 'en';
  entitlement: AiEntitlement | null;
  onClose: () => void;
  onChanged: (entitlement: AiEntitlement) => void;
}

export function AiBillingDialog({ locale, entitlement, onClose, onChanged }: AiBillingDialogProps) {
  const t = copy[locale];
  const [busy, setBusy] = useState<'personal' | 'high' | 'open' | 'activate' | 'deactivate' | 'reveal' | null>(null);
  const [selectedPlan, setSelectedPlan] = useState<'personal' | 'high'>('personal');
  const [order, setOrder] = useState<AiBillingOrder | null>(null);
  const [licenseKey, setLicenseKey] = useState('');
  const [revealedLicenseKey, setRevealedLicenseKey] = useState<string | null>(null);
  const [licenseError, setLicenseError] = useState('');
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [confirmingDeactivate, setConfirmingDeactivate] = useState(false);
  const planName = entitlement ? t[entitlement.plan] : t.free;
  const expiredPlan = entitlement?.expired ? entitlement.previousPlan : null;
  const upgrading = !!entitlement?.licensed && entitlement.plan === 'personal' && selectedPlan === 'high';
  const downgradeBlocked = !!entitlement?.licensed && entitlement.plan === 'high' && selectedPlan === 'personal';
  const actionLabel = expiredPlan ? t.renewNow : upgrading ? t.upgradeNow : entitlement?.licensed ? t.renew : t.buy;
  const price = (plan: 'personal' | 'high') => entitlement?.paymentAmountOverride ?? (plan === 'personal' ? 9.9 : 49);
  const formatPrice = (value: number) => Number.isInteger(value) ? String(value) : value.toFixed(2).replace(/0$/, '');
  const formatDate = (value: string | null | undefined) => value ? new Date(value).toLocaleDateString(locale === 'zh' ? 'zh-CN' : 'en-US') : '—';

  useEffect(() => {
    const preferredPlan = entitlement?.licensed && entitlement.plan !== 'free' ? entitlement.plan : expiredPlan;
    if (preferredPlan) setSelectedPlan(preferredPlan);
  }, [entitlement?.licensed, entitlement?.plan, expiredPlan]);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busy) {
        event.preventDefault();
        if (confirmingDeactivate) setConfirmingDeactivate(false);
        else onClose();
      }
    };
    document.addEventListener('keydown', closeOnEscape, true);
    return () => document.removeEventListener('keydown', closeOnEscape, true);
  }, [busy, confirmingDeactivate, onClose]);

  const openCheckout = async (checkoutOrder: AiBillingOrder) => {
    await desktop.openExternal(checkoutOrder.checkoutUrl);
    setOrder(checkoutOrder);
    setNotice(t.orderCreated);
  };
  const createOrder = async (plan: 'personal' | 'high') => {
    setBusy(plan); setError(''); setNotice(''); setConfirmingDeactivate(false);
    try {
      const created = await desktop.aiCreateOrder(plan);
      await openCheckout(created);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setBusy(null); }
  };
  const reopenCheckout = async () => {
    if (!order) return;
    setBusy('open'); setError(''); setNotice('');
    try { await openCheckout(order); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(null); }
  };
  const toggleLicense = async () => {
    if (revealedLicenseKey) { setRevealedLicenseKey(null); return; }
    setBusy('reveal'); setLicenseError(''); setError(''); setNotice('');
    try {
      const key = await desktop.aiLicenseKey();
      if (!key) throw new Error(t.noLicense);
      setRevealedLicenseKey(key);
    } catch (reason) {
      setLicenseError(reason instanceof Error ? reason.message : String(reason));
    } finally { setBusy(null); }
  };
  const copyCurrentLicense = async () => {
    setBusy('reveal'); setLicenseError(''); setError(''); setNotice('');
    try {
      const key = revealedLicenseKey ?? await desktop.aiLicenseKey();
      if (!key) throw new Error(t.noLicense);
      await navigator.clipboard.writeText(key);
      setNotice(t.licenseCopied);
    } catch (reason) {
      setLicenseError(reason instanceof Error ? reason.message : String(reason));
    } finally { setBusy(null); }
  };
  const activate = async () => {
    if (!licenseKey.trim()) return;
    setBusy('activate'); setError(''); setNotice('');
    try {
      const next = await desktop.aiActivateLicense(licenseKey.trim());
      onChanged(next);
      setLicenseKey(''); setOrder(null); setRevealedLicenseKey(null); setNotice(t.activated);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setBusy(null); }
  };
  const deactivate = async () => {
    setBusy('deactivate'); setError(''); setNotice('');
    try {
      const next = await desktop.aiDeactivateLicense();
      onChanged(next); setConfirmingDeactivate(false); setRevealedLicenseKey(null); setNotice(t.deactivated);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setBusy(null); }
  };

  return <div className="modal-backdrop ai-billing-backdrop" onMouseDown={(event) => {
    if (!busy && event.target === event.currentTarget) onClose();
  }}>
    <section className="modal ai-billing-modal" role="dialog" aria-modal="true" aria-labelledby="ai-billing-title">
      <header><h2 id="ai-billing-title"><CreditCard size={17} />{t.title}</h2><button className="icon" disabled={!!busy} onClick={onClose} aria-label={t.close}><X size={17} /></button></header>
      <div className="ai-billing-content">
        <section className="billing-current">
          <div className="billing-current-heading"><div><small>{t.currentPlan}</small><h3>{planName}</h3></div><span className={entitlement?.bound ? 'bound' : ''}><Laptop size={13} />{entitlement?.bound ? t.bound : t.unbound}</span></div>
          <div className="billing-quota"><strong>{entitlement?.remaining ?? '—'}</strong><span>{t.remaining}<small>{entitlement ? `${entitlement.used} / ${entitlement.quota} ${t.times}` : t.monthlyReset}</small></span></div>
          <div className="billing-period"><span>{entitlement?.licensed ? `${t.validUntil} ${formatDate(entitlement.periodEnd)}` : t.monthlyReset}</span><span>{t.independent}</span></div>
        </section>
        {expiredPlan && <div className="billing-expired" role="alert"><AlertTriangle size={16} /><span><strong>{t[expiredPlan]} · {t.expired}</strong><small>{t.expiredAt}：{formatDate(entitlement?.previousPeriodEnd)}</small></span></div>}
        {entitlement?.bound && <div className="billing-license-group">
          <section className="billing-saved-license" aria-label={t.currentLicense}>
            <div><small>{t.currentLicense}</small><code>{revealedLicenseKey ?? t.maskedLicense}</code></div>
            <div className="billing-saved-license-actions">
              <button className="button secondary" disabled={!!busy} onClick={() => void toggleLicense()}>{busy === 'reveal' ? <LoaderCircle className="spin" size={13} /> : revealedLicenseKey ? <EyeOff size={13} /> : <Eye size={13} />}{revealedLicenseKey ? t.hideLicense : t.showLicense}</button>
              <button className="button secondary" disabled={!!busy} onClick={() => void copyCurrentLicense()}><Copy size={13} />{t.copyLicense}</button>
            </div>
          </section>
          {licenseError && <p className="error billing-license-error" role="alert">{licenseError || t.requestFailed}</p>}
        </div>}

        {(entitlement?.mockPayment || order?.mock) && <p className="billing-mock"><AlertTriangle size={14} />{t.mock}</p>}
        <section className="billing-section">
          <h3>{t.choose}</h3>
          <div className="billing-plans" role="radiogroup" aria-label={t.choose}>
            {(['personal', 'high'] as const).map((plan) => <button type="button" role="radio" aria-checked={selectedPlan === plan} className={`billing-plan ${plan} ${selectedPlan === plan ? 'selected' : ''}`} key={plan} disabled={!!busy} onClick={() => {
              setSelectedPlan(plan);
              setOrder((current) => current?.plan === plan ? current : null);
              setError(''); setNotice('');
            }}>
              <div><strong>{t[plan]}</strong><p><b>¥{formatPrice(price(plan))}</b> / {t.month}</p><span>{plan === 'personal' ? t.personalDetail : t.highDetail}</span></div>
              {selectedPlan === plan && <Check className="billing-plan-check" size={16} />}
            </button>)}
          </div>
          {upgrading && <p className="billing-upgrade"><ShieldCheck size={14} />{t.upgradeCarry}</p>}
          {downgradeBlocked && <p className="billing-help">{t.downgradeBlocked}</p>}
          <div className="billing-plan-actions"><button className="button primary" disabled={!!busy || !entitlement?.paymentEnabled || downgradeBlocked} onClick={() => void createOrder(selectedPlan)}>
            {busy === selectedPlan ? <LoaderCircle className="spin" size={14} /> : <CreditCard size={14} />}{actionLabel} · {t[selectedPlan]} ¥{formatPrice(price(selectedPlan))}
          </button></div>
          {!entitlement?.paymentEnabled && <p className="billing-help">{t.paymentUnavailable}</p>}
          {order && <div className="billing-order"><span><small>{t.order} · {order.status === 'paid' ? t.paid : t.pending}</small><strong>{t[order.plan]} · ¥{order.amount}</strong></span><button className="button secondary" disabled={!!busy} onClick={() => void reopenCheckout()}><ExternalLink size={13} />{busy === 'open' ? t.opening : t.reopen}</button></div>}
        </section>

        <section className="billing-section billing-activation">
          <h3><KeyRound size={15} />{t.activateTitle}</h3>
          <div className="billing-license-field"><input autoCapitalize="characters" autoCorrect="off" spellCheck={false} maxLength={64} value={licenseKey} onChange={(event) => setLicenseKey(event.target.value.toUpperCase())} placeholder={t.licensePlaceholder} aria-label={t.licensePlaceholder} /><button className="button primary" disabled={!!busy || !licenseKey.trim()} onClick={() => void activate()}>{busy === 'activate' ? <LoaderCircle className="spin" size={14} /> : <Check size={14} />}{busy === 'activate' ? t.activating : t.activate}</button></div>
          <p className="billing-help">{t.licenseHelp}</p>
        </section>

        {entitlement?.bound && <section className="billing-device-section">
          {!confirmingDeactivate ? <button className="billing-unbind" disabled={!!busy} onClick={() => { setConfirmingDeactivate(true); setRevealedLicenseKey(null); setError(''); setNotice(''); }}>{t.deactivate}</button> : <div className="billing-unbind-confirm" role="alert"><strong>{t.deactivateTitle}</strong><p>{t.deactivateHelp}</p><div><button className="button secondary" disabled={!!busy} onClick={() => setConfirmingDeactivate(false)}>{t.cancel}</button><button className="button danger" disabled={!!busy} onClick={() => void deactivate()}>{busy === 'deactivate' ? t.deactivating : t.confirmDeactivate}</button></div></div>}
        </section>}
        {notice && <p className="billing-notice" role="status"><ShieldCheck size={14} />{notice}</p>}
        {error && <p className="error" role="alert">{error || t.requestFailed}</p>}
        <p className="billing-security"><ShieldCheck size={13} />{t.security}</p>
      </div>
    </section>
  </div>;
}
