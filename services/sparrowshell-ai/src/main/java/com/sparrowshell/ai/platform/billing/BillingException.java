package com.sparrowshell.ai.platform.billing;

public class BillingException extends RuntimeException {
    public final String code;
    public BillingException(String code, String message) { super(message); this.code = code; }
}
