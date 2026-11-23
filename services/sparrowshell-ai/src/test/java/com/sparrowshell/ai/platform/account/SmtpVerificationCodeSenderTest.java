package com.sparrowshell.ai.platform.account;

import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.mail.SimpleMailMessage;
import org.springframework.mail.javamail.JavaMailSender;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

class SmtpVerificationCodeSenderTest {
    @Test public void missingLocaleKeepsChineseRegistrationEmail() {
        JavaMailSender mailSender = mock(JavaMailSender.class);
        sender(mailSender).send("person@example.test", "123456", "register", null);

        SimpleMailMessage message = sentMessage(mailSender);
        assertEquals("SparrowShell <verify@example.com>", message.getFrom());
        assertEquals("SparrowShell 注册验证码", message.getSubject());
        assertTrue(message.getText().contains("你的验证码是：123456"));
        assertTrue(message.getText().contains("验证码 5 分钟内有效"));
    }

    @Test public void englishLocaleUsesEnglishRegistrationSubjectAndBody() {
        JavaMailSender mailSender = mock(JavaMailSender.class);
        sender(mailSender).send("person@example.test", "123456", "register", "en-US");

        SimpleMailMessage message = sentMessage(mailSender);
        assertEquals("SparrowShell Verification Code", message.getSubject());
        assertEquals("Your verification code is: 123456\n\nThis code is valid for 5 minutes and expires after successful verification. "
                + "If you did not request this code, ignore this email. Do not share the code with anyone.", message.getText());
        assertFalse(message.getText().contains("验证码"));
    }

    @Test public void englishLocaleUsesEnglishPasswordResetSubject() {
        JavaMailSender mailSender = mock(JavaMailSender.class);
        sender(mailSender).send("person@example.test", "123456", "password_reset", "en-US");

        SimpleMailMessage message = sentMessage(mailSender);
        assertEquals("SparrowShell Password Reset Code", message.getSubject());
        assertTrue(message.getText().startsWith("Your verification code is: 123456"));
    }

    private static SmtpVerificationCodeSender sender(JavaMailSender mailSender) {
        return new SmtpVerificationCodeSender(mailSender, "verify@example.com");
    }

    private static SimpleMailMessage sentMessage(JavaMailSender mailSender) {
        ArgumentCaptor<SimpleMailMessage> captor = ArgumentCaptor.forClass(SimpleMailMessage.class);
        verify(mailSender).send(captor.capture());
        return captor.getValue();
    }
}
