package com.sparrowshell.ai;

public class ProviderException extends RuntimeException {
    private final String code;
    public ProviderException(String code, String safeMessage) { super(safeMessage); this.code = code; }
    public String code() { return code; }
}
