package com.sparrowshell.ai.platform.account;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.autoconfigure.condition.ConditionalOnExpression;
import org.springframework.stereotype.Component;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.time.Clock;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;

@Component
@ConditionalOnExpression("'${sparrowshell.mail.enabled:false}' == 'true' && '${sparrowshell.mail.provider:smtp}' == 'tencent-ses'")
class TencentSesVerificationCodeSender implements VerificationCodeSender {
    private static final Logger LOGGER = LoggerFactory.getLogger(TencentSesVerificationCodeSender.class);
    private static final String HOST = "ses.tencentcloudapi.com";
    private static final String ENDPOINT = "https://" + HOST + "/";
    private static final ObjectMapper JSON = new ObjectMapper();

    private final String secretId;
    private final String secretKey;
    private final String region;
    private final String from;
    private final long templateId;
    private final SesTransport transport;
    private final Clock clock;

    @Autowired
    TencentSesVerificationCodeSender(
            @Value("${sparrowshell.ses.secret-id}") String secretId,
            @Value("${sparrowshell.ses.secret-key}") String secretKey,
            @Value("${sparrowshell.ses.region:ap-guangzhou}") String region,
            @Value("${sparrowshell.mail.from}") String from,
            @Value("${sparrowshell.ses.template-id}") long templateId) {
        this(secretId, secretKey, region, from, templateId, new HttpSesTransport(), Clock.systemUTC());
    }

    TencentSesVerificationCodeSender(String secretId, String secretKey, String region, String from,
            long templateId, SesTransport transport, Clock clock) {
        this.secretId = secretId;
        this.secretKey = secretKey;
        this.region = region;
        this.from = from;
        this.templateId = templateId;
        this.transport = transport;
        this.clock = clock;
    }

    @Override public void send(String email, String code, String purpose, String locale) {
        try {
            boolean registration = "register".equals(purpose);
            boolean english = locale != null && locale.toLowerCase(Locale.ROOT).startsWith("en");
            Map<String, Object> template = new LinkedHashMap<String, Object>();
            template.put("TemplateID", templateId);
            template.put("TemplateData", JSON.writeValueAsString(Collections.singletonMap("code", code)));
            Map<String, Object> request = new LinkedHashMap<String, Object>();
            request.put("FromEmailAddress", "SparrowShell <" + from + ">");
            request.put("Destination", Collections.singletonList(email));
            request.put("Template", template);
            request.put("Subject", english
                    ? (registration ? "SparrowShell Verification Code" : "SparrowShell Password Reset Code")
                    : (registration ? "SparrowShell 注册验证码" : "SparrowShell 密码重置验证码"));
            String payload = JSON.writeValueAsString(request);
            long timestamp = clock.instant().getEpochSecond();
            Map<String, String> headers = new LinkedHashMap<String, String>();
            headers.put("Authorization", TencentCloudV3Signer.authorization(secretId, secretKey, "ses", HOST, payload, timestamp));
            headers.put("Content-Type", "application/json; charset=utf-8");
            headers.put("X-TC-Action", "SendEmail");
            headers.put("X-TC-Timestamp", String.valueOf(timestamp));
            headers.put("X-TC-Version", "2020-10-02");
            headers.put("X-TC-Region", region);
            String response = transport.post(payload, headers);
            JsonNode error = JSON.readTree(response).path("Response").path("Error");
            if (!error.isMissingNode()) {
                LOGGER.warn("Tencent SES rejected a verification email with code {}", error.path("Code").asText("unknown"));
                throw unavailable();
            }
        } catch (AccountException exception) {
            throw exception;
        } catch (Exception exception) {
            LOGGER.warn("Tencent SES verification email failed: {}", exception.getMessage());
            throw unavailable();
        }
    }

    private static AccountException unavailable() {
        return new AccountException("mail_unavailable", "验证码邮件发送失败，请稍后重试");
    }

    interface SesTransport {
        String post(String payload, Map<String, String> headers) throws Exception;
    }

    private static final class HttpSesTransport implements SesTransport {
        @Override public String post(String payload, Map<String, String> headers) throws IOException {
            HttpURLConnection connection = (HttpURLConnection) new URL(ENDPOINT).openConnection();
            try {
                connection.setRequestMethod("POST");
                connection.setConnectTimeout(10_000);
                connection.setReadTimeout(15_000);
                connection.setInstanceFollowRedirects(false);
                connection.setUseCaches(false);
                connection.setDoOutput(true);
                for (Map.Entry<String, String> header : headers.entrySet()) {
                    connection.setRequestProperty(header.getKey(), header.getValue());
                }
                byte[] body = payload.getBytes(StandardCharsets.UTF_8);
                connection.setFixedLengthStreamingMode(body.length);
                try (OutputStream output = connection.getOutputStream()) {
                    output.write(body);
                }
                int status = connection.getResponseCode();
                if (status < 200 || status >= 300) throw new IOException("SES HTTP status " + status);
                try (InputStream input = connection.getInputStream()) {
                    return readBounded(input);
                }
            } finally {
                connection.disconnect();
            }
        }

        private static String readBounded(InputStream input) throws IOException {
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            byte[] buffer = new byte[4096];
            int read;
            while ((read = input.read(buffer)) >= 0) {
                if (output.size() + read > 65_536) throw new IOException("SES response is too large");
                output.write(buffer, 0, read);
            }
            return new String(output.toByteArray(), StandardCharsets.UTF_8);
        }
    }
}
