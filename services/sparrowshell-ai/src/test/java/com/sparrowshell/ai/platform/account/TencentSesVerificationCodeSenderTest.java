package com.sparrowshell.ai.platform.account;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;

import java.time.Clock;
import java.time.Instant;
import java.time.ZoneOffset;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

class TencentSesVerificationCodeSenderTest {
    private static final ObjectMapper JSON = new ObjectMapper();

    @Test public void sendsTemplateCodeWithOfficialTc3Signature() throws Exception {
        CapturingTransport transport = new CapturingTransport();
        sender(transport).send("user@example.com", "123456", "register", "zh-CN");

        JsonNode request = JSON.readTree(transport.payload);
        assertEquals("SparrowShell <verify@example.com>", request.path("FromEmailAddress").asText());
        assertEquals("user@example.com", request.path("Destination").get(0).asText());
        assertEquals(100001, request.path("Template").path("TemplateID").asLong());
        assertEquals("{\"code\":\"123456\"}", request.path("Template").path("TemplateData").asText());
        assertEquals("SparrowShell 注册验证码", request.path("Subject").asText());
        assertEquals("SendEmail", transport.headers.get("X-TC-Action"));
        assertEquals("2020-10-02", transport.headers.get("X-TC-Version"));
        assertEquals("ap-guangzhou", transport.headers.get("X-TC-Region"));
        assertEquals("1793930400", transport.headers.get("X-TC-Timestamp"));
        assertEquals("TC3-HMAC-SHA256 Credential=AKIDTEST/2026-11-06/ses/tc3_request, "
                + "SignedHeaders=content-type;host, Signature=1d99a11437e993b9c596cce79ed4724e4136557236613427d57c62125beaa566",
                transport.headers.get("Authorization"));
    }

    @Test public void passwordResetUsesResetSubject() throws Exception {
        CapturingTransport transport = new CapturingTransport();
        sender(transport).send("user@example.com", "123456", "password_reset", "zh-CN");
        assertEquals("SparrowShell 密码重置验证码", JSON.readTree(transport.payload).path("Subject").asText());
    }

    @Test public void englishLocaleUsesEnglishSubject() throws Exception {
        CapturingTransport transport = new CapturingTransport();
        sender(transport).send("user@example.com", "123456", "password_reset", "en-US");
        assertEquals("SparrowShell Password Reset Code", JSON.readTree(transport.payload).path("Subject").asText());
    }

    @Test public void sesErrorIsReportedAsMailUnavailable() {
        CapturingTransport transport = new CapturingTransport();
        transport.response = "{\"Response\":{\"Error\":{\"Code\":\"InvalidParameterValue\"}}}";
        AccountException error = assertThrows(AccountException.class,
                () -> sender(transport).send("user@example.com", "123456", "register", "zh-CN"));
        assertEquals("mail_unavailable", error.code);
    }

    @Test public void transportFailureIsReportedAsMailUnavailable() {
        CapturingTransport transport = new CapturingTransport();
        transport.failure = new java.io.IOException("offline");
        AccountException error = assertThrows(AccountException.class,
                () -> sender(transport).send("user@example.com", "123456", "register", "zh-CN"));
        assertEquals("mail_unavailable", error.code);
    }

    private static TencentSesVerificationCodeSender sender(CapturingTransport transport) {
        return new TencentSesVerificationCodeSender("AKIDTEST", "test-secret", "ap-guangzhou",
                "verify@example.com", 100001, transport,
                Clock.fixed(Instant.ofEpochSecond(1793930400L), ZoneOffset.UTC));
    }

    private static final class CapturingTransport implements TencentSesVerificationCodeSender.SesTransport {
        String payload;
        Map<String, String> headers;
        String response = "{\"Response\":{\"MessageId\":\"message-id\"}}";
        Exception failure;

        @Override public String post(String payload, Map<String, String> headers) throws Exception {
            this.payload = payload;
            this.headers = headers;
            if (failure != null) throw failure;
            return response;
        }
    }
}
