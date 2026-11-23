package com.sparrowshell.ai.platform.account;

import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.autoconfigure.condition.ConditionalOnExpression;
import org.springframework.mail.SimpleMailMessage;
import org.springframework.mail.javamail.JavaMailSender;
import org.springframework.stereotype.Component;

import java.util.Locale;

@Component
@ConditionalOnExpression("'${sparrowshell.mail.enabled:false}' == 'true' && '${sparrowshell.mail.provider:smtp}' == 'smtp'")
class SmtpVerificationCodeSender implements VerificationCodeSender {
    private final JavaMailSender mailSender;
    private final String from;

    SmtpVerificationCodeSender(JavaMailSender mailSender,
            @Value("${sparrowshell.mail.from}") String from) {
        this.mailSender = mailSender;
        this.from = from;
    }

    @Override public void send(String email, String code, String purpose, String locale) {
        boolean registration = "register".equals(purpose);
        boolean english = locale != null && locale.toLowerCase(Locale.ROOT).startsWith("en");
        SimpleMailMessage message = new SimpleMailMessage();
        message.setFrom("SparrowShell <" + from + ">");
        message.setTo(email);
        message.setSubject(english
                ? (registration ? "SparrowShell Verification Code" : "SparrowShell Password Reset Code")
                : (registration ? "SparrowShell 注册验证码" : "SparrowShell 密码重置验证码"));
        message.setText(english
                ? "Your verification code is: " + code + "\n\nThis code is valid for 5 minutes and expires after successful verification. "
                        + "If you did not request this code, ignore this email. Do not share the code with anyone."
                : "你的验证码是：" + code + "\n\n验证码 5 分钟内有效，验证成功后立即失效。"
                        + "如果这不是你的操作，请忽略此邮件，不要将验证码告诉任何人。");
        try {
            mailSender.send(message);
        } catch (RuntimeException exception) {
            throw new AccountException("mail_unavailable", "验证码邮件发送失败，请稍后重试");
        }
    }
}
