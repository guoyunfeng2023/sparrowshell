package com.sparrowshell.ai.platform.admin;

import org.springframework.http.HttpStatus;
import org.springframework.stereotype.Component;
import org.springframework.web.server.ResponseStatusException;
import javax.servlet.http.HttpServletRequest;

/** Public package boundary for legacy controllers during the gradual RuoYi migration. */
@Component
public class AdminAccessGuard {
    private final AdminAuthService auth;
    AdminAccessGuard(AdminAuthService auth) { this.auth = auth; }

    public void requireAiRead(HttpServletRequest request) {
        try {
            auth.requireRole(AdminAuthFilter.session(request), "SUPER_ADMIN", "SUPPORT_ADMIN", "VIEWER", "AI_ADMIN");
        } catch (AdminException exception) {
            if ("permission_denied".equals(exception.code))
                throw new ResponseStatusException(HttpStatus.FORBIDDEN, "当前管理员角色无权查看 AI 数据");
            throw exception;
        }
    }
}
