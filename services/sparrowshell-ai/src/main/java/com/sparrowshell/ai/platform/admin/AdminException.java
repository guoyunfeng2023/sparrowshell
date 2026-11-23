package com.sparrowshell.ai.platform.admin;

class AdminException extends RuntimeException {
    final String code;
    AdminException(String code, String message) { super(message); this.code = code; }
    static AdminException unavailable() { return new AdminException("unavailable", "管理端持久化存储尚未配置"); }
    static AdminException unauthorized() { return new AdminException("authentication_required", "管理员会话无效或已过期"); }
    static AdminException conflict() { return new AdminException("revision_conflict", "数据已被其他管理员修改，请刷新后重试"); }
}
