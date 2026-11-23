package com.sparrowshell.ai.platform.feedback;

public class FeedbackException extends RuntimeException {
    public final String code;
    public FeedbackException(String code, String message) { super(message); this.code = code; }
}
