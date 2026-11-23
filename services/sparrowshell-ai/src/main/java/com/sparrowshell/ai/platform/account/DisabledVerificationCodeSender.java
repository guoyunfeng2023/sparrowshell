package com.sparrowshell.ai.platform.account;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Component;

@Component
@ConditionalOnProperty(prefix = "sparrowshell.mail", name = "enabled", havingValue = "false", matchIfMissing = true)
class DisabledVerificationCodeSender implements VerificationCodeSender {
    @Override public void send(String email, String code, String purpose, String locale) {
        throw new AccountException("mail_unavailable", "邮件验证码服务尚未配置，请联系管理员");
    }
}
