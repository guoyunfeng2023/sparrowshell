package com.sparrowshell.ai.platform.account;

interface VerificationCodeSender {
    void send(String email, String code, String purpose, String locale);
}
