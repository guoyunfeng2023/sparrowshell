import { X } from 'lucide-react';
import type { Locale } from '../../i18n';

export type LegalDocument = 'terms' | 'privacy';

const documents = {
  zh: {
    terms: {
      title: '服务条款',
      sections: [
        ['本地工具与责任', 'SparrowShell 是本地 SSH 与运维辅助工具。你应仅连接和操作有权管理的服务器，并在执行命令前核对目标、内容和影响。自动执行所有命令模式不会追加确认，请谨慎启用。'],
        ['账号与云同步', '不登录也可使用本地功能。登录后，文件夹、非敏感服务器配置和自动化定义可在你的设备间同步；服务器密码、私钥和私钥口令不会同步。你应妥善保管账号与本机钥匙串。'],
        ['AI 辅助', '模型生成的命令或分析可能不准确。实际命令由本地执行能力运行，你仍需对授权范围和执行结果负责。请勿提交无权披露的信息。'],
        ['反馈与更新', '问题反馈只包含你主动填写的描述、联系方式和选择的附件。检查更新只用于获取适用版本；下载或安装前仍会展示版本信息，不会静默安装或重启。'],
      ],
    },
    privacy: {
      title: '隐私政策',
      sections: [
        ['本地数据', '服务器配置、命令历史和偏好默认保存在本机。服务器密码和私钥口令使用 macOS 钥匙串保护；登录令牌、AI 激活码及配套设备令牌仅保存在本机客户端，均不参与云同步。'],
        ['云同步数据', '登录后，为提供多设备同步，服务端会保存账号标识、设备会话及非敏感配置。你可以退出登录或注销云账号；本地数据不会因退出登录而删除。'],
        ['AI 请求', '使用 AI 时，问题及你授权的上下文会发送给配置的模型服务。密码、私钥、口令和令牌不会作为上下文发送。请避免在问题或附件中主动包含敏感信息。'],
        ['问题反馈', '未登录时反馈匿名保存；已登录时，服务端根据认证令牌关联用户 ID，而不要求你重复填写身份信息。反馈仅保存问题描述、选填联系方式、选填附件及处理状态，不自动收集终端、服务器配置或 AI 会话。'],
        ['检查更新', '检查更新会发送当前版本、稳定渠道、本机设备标识和 darwin-aarch64 目标，用于判断是否有适用版本，不要求登录。'],
      ],
    },
    close: '关闭',
  },
  en: {
    terms: {
      title: 'Terms of Service',
      sections: [
        ['Local tool and responsibility', 'SparrowShell is a local SSH and operations assistant. Connect only to servers you are authorized to manage, and verify the target, command and impact before execution. Full-auto mode runs every command without additional confirmation.'],
        ['Accounts and cloud sync', 'Local features work without an account. When signed in, folders, non-sensitive server configurations and automation definitions can sync across your devices. Server passwords, private keys and passphrases are never synced.'],
        ['AI assistance', 'Model-generated commands and analysis may be inaccurate. Commands run through local execution capabilities, and you remain responsible for granted permissions and their results. Do not submit information you are not authorized to disclose.'],
        ['Feedback and updates', 'Feedback contains only the description, optional contact and attachment you explicitly provide. Update checks retrieve applicable release information and never silently install or restart the app.'],
      ],
    },
    privacy: {
      title: 'Privacy Policy',
      sections: [
        ['Local data', 'Server configurations, command history and preferences are stored on this Mac by default. Server passwords and passphrases are protected by macOS Keychain. Account tokens, AI licenses and paired AI device tokens are stored only in the local app and are never synced.'],
        ['Cloud sync data', 'After sign-in, the service stores account identifiers, device sessions and non-sensitive configurations to provide multi-device sync. Signing out does not delete local data.'],
        ['AI requests', 'AI requests contain your question and the context you authorize. Passwords, private keys, passphrases and tokens are never sent as context. Avoid including sensitive information in prompts or attachments.'],
        ['Feedback', 'Anonymous feedback has no user ID. For signed-in users, the service associates the user ID from the authentication token. Only the description, optional contact, optional attachment and processing status are stored; terminal, server configuration and AI conversations are not collected automatically.'],
        ['Update checks', 'Update checks send the current version, stable channel, local device identifier and darwin-aarch64 target to find an applicable release. No sign-in is required.'],
      ],
    },
    close: 'Close',
  },
} as const;

export function LegalDialog({ locale, document, onClose }: { locale: Locale; document: LegalDocument; onClose: () => void }) {
  const content = documents[locale][document];
  return <div className="modal-backdrop legal-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <section className="modal legal-modal" role="dialog" aria-modal="true" aria-label={content.title}>
      <header><h2>{content.title}</h2><button className="icon" onClick={onClose} aria-label={documents[locale].close}><X size={17} /></button></header>
      <div className="legal-content">{content.sections.map(([title, body]) => <section key={title}><h3>{title}</h3><p>{body}</p></section>)}</div>
    </section>
  </div>;
}
