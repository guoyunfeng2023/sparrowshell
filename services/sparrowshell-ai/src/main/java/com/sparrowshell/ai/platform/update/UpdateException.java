package com.sparrowshell.ai.platform.update;

public class UpdateException extends RuntimeException {
    public final String code;
    public UpdateException(String code, String message) { super(message); this.code = code; }
}
