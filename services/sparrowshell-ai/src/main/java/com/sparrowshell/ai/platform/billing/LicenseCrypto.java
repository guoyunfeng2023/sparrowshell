package com.sparrowshell.ai.platform.billing;

import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Component;
import javax.crypto.Cipher;
import javax.crypto.spec.GCMParameterSpec;
import javax.crypto.spec.SecretKeySpec;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.util.Base64;

@Component
class LicenseCrypto {
    private static final SecureRandom RANDOM = new SecureRandom();
    private final byte[] key;
    LicenseCrypto(@Value("${sparrowshell.billing.license-key-secret:}") String secret,
                  @Value("${sparrowshell.billing.payment-channel:disabled}") String channel) {
        String value = secret == null ? "" : secret.trim();
        if (value.isEmpty() && "mock".equals(channel)) value = "SparrowShell isolated development license encryption";
        this.key = value.isEmpty() ? null : sha256(value.getBytes(StandardCharsets.UTF_8));
    }
    String encrypt(String plaintext) {
        if (key == null) throw new BillingException("payment_unavailable", "许可证加密密钥尚未配置");
        try {
            byte[] iv = new byte[12]; RANDOM.nextBytes(iv);
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding"); cipher.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(key, "AES"), new GCMParameterSpec(128, iv));
            byte[] encrypted = cipher.doFinal(plaintext.getBytes(StandardCharsets.UTF_8));
            byte[] result = new byte[iv.length + encrypted.length]; System.arraycopy(iv, 0, result, 0, iv.length);
            System.arraycopy(encrypted, 0, result, iv.length, encrypted.length); return Base64.getEncoder().encodeToString(result);
        } catch (Exception exception) { throw new BillingException("payment_unavailable", "许可证安全存储失败"); }
    }
    String decrypt(String ciphertext) {
        if (key == null) throw new BillingException("payment_unavailable", "许可证加密密钥尚未配置");
        try {
            byte[] input = Base64.getDecoder().decode(ciphertext); if (input.length < 29) throw new IllegalArgumentException();
            byte[] iv = java.util.Arrays.copyOfRange(input, 0, 12); byte[] encrypted = java.util.Arrays.copyOfRange(input, 12, input.length);
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding"); cipher.init(Cipher.DECRYPT_MODE, new SecretKeySpec(key, "AES"), new GCMParameterSpec(128, iv));
            return new String(cipher.doFinal(encrypted), StandardCharsets.UTF_8);
        } catch (Exception exception) { throw new BillingException("license_unavailable", "许可证暂时无法读取"); }
    }
    static String randomHex(int bytes) { byte[] value = new byte[bytes]; RANDOM.nextBytes(value); StringBuilder text = new StringBuilder(); for (byte item : value) text.append(String.format("%02x", item & 0xff)); return text.toString(); }
    static String newLicenseKey() { String hex = randomHex(16).toUpperCase(); StringBuilder key = new StringBuilder("SS"); for (int i = 0; i < hex.length(); i += 4) key.append('-').append(hex, i, i + 4); return key.toString(); }
    static String normalizeLicenseKey(String value) { return value == null ? "" : value.trim().toUpperCase(); }
    private static byte[] sha256(byte[] value) { try { return MessageDigest.getInstance("SHA-256").digest(value); } catch (Exception exception) { throw new IllegalStateException(exception); } }
}
