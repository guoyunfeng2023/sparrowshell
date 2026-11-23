package com.sparrowshell.ai.platform.job;

public class JobFailureException extends Exception {
    public final String code;
    public final boolean retryable;
    public JobFailureException(String code, boolean retryable) {
        super(code);
        this.code = code;
        this.retryable = retryable;
    }
}
