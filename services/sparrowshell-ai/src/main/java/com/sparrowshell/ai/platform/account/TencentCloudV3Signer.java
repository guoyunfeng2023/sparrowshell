package com.sparrowshell.ai.platform.account;

import javax.crypto.Mac;
import javax.crypto.spec.SecretKeySpec;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.security.MessageDigest;
import java.time.Instant;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;

/** Tencent Cloud API 3.0 TC3-HMAC-SHA256 request signing. */
final class TencentCloudV3Signer {
    private static final String ALGORITHM = "TC3-HMAC-SHA256";
    private static final String CONTENT_TYPE = "application/json; charset=utf-8";
    private static final DateTimeFormatter DATE = DateTimeFormatter.ISO_LOCAL_DATE.withZone(ZoneOffset.UTC);

    private TencentCloudV3Signer() { }

    static String authorization(String secretId, String secretKey, String service, String host,
            String payload, long timestamp) throws GeneralSecurityException {
        String date = DATE.format(Instant.ofEpochSecond(timestamp));
        String canonicalHeaders = "content-type:" + CONTENT_TYPE + "\nhost:" + host + "\n";
        String canonicalRequest = "POST\n/\n\n" + canonicalHeaders + "\ncontent-type;host\n" + sha256Hex(payload);
        String credentialScope = date + "/" + service + "/tc3_request";
        String stringToSign = ALGORITHM + "\n" + timestamp + "\n" + credentialScope + "\n" + sha256Hex(canonicalRequest);
        byte[] secretDate = hmac(("TC3" + secretKey).getBytes(StandardCharsets.UTF_8), date);
        byte[] secretService = hmac(secretDate, service);
        byte[] secretSigning = hmac(secretService, "tc3_request");
        String signature = hex(hmac(secretSigning, stringToSign));
        return ALGORITHM + " Credential=" + secretId + "/" + credentialScope
                + ", SignedHeaders=content-type;host, Signature=" + signature;
    }

    private static byte[] hmac(byte[] key, String value) throws GeneralSecurityException {
        Mac mac = Mac.getInstance("HmacSHA256");
        mac.init(new SecretKeySpec(key, "HmacSHA256"));
        return mac.doFinal(value.getBytes(StandardCharsets.UTF_8));
    }

    private static String sha256Hex(String value) throws GeneralSecurityException {
        return hex(MessageDigest.getInstance("SHA-256").digest(value.getBytes(StandardCharsets.UTF_8)));
    }

    private static String hex(byte[] bytes) {
        StringBuilder result = new StringBuilder(bytes.length * 2);
        for (byte value : bytes) result.append(String.format("%02x", value & 0xff));
        return result.toString();
    }
}
