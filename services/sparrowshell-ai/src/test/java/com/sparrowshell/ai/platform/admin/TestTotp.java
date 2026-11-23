package com.sparrowshell.ai.platform.admin;

import javax.crypto.Mac;
import javax.crypto.spec.SecretKeySpec;
import java.nio.ByteBuffer;
import java.time.Instant;

final class TestTotp {
    private TestTotp() { }
    static String at(String secret, Instant instant) {
        try {
            String alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
            java.io.ByteArrayOutputStream output = new java.io.ByteArrayOutputStream();
            int buffer = 0, bits = 0;
            for (char c : secret.toCharArray()) {
                buffer = (buffer << 5) | alphabet.indexOf(c); bits += 5;
                if (bits >= 8) { bits -= 8; output.write((buffer >> bits) & 255); }
            }
            Mac mac = Mac.getInstance("HmacSHA1"); mac.init(new SecretKeySpec(output.toByteArray(), "HmacSHA1"));
            byte[] hash = mac.doFinal(ByteBuffer.allocate(8).putLong(instant.getEpochSecond() / 30).array());
            int offset = hash[19] & 15;
            int value = ((hash[offset] & 127) << 24) | ((hash[offset + 1] & 255) << 16) | ((hash[offset + 2] & 255) << 8) | (hash[offset + 3] & 255);
            return String.format("%06d", value % 1_000_000);
        } catch (Exception exception) { throw new AssertionError(exception); }
    }
}
