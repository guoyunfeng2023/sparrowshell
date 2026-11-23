package com.sparrowshell.ai.platform.account;

public class AccountException extends RuntimeException {
    public final String code;
    AccountException(String code, String message) { super(message); this.code = code; }
    static AccountException unauthorized() { return new AccountException("invalid_credentials", "邮箱、密码或会话无效"); }
    static AccountException unavailable() { return new AccountException("unavailable", "账号与同步服务尚未配置持久化"); }
}
