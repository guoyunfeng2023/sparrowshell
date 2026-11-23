package com.sparrowshell.ai.platform.admin;

import org.springframework.http.MediaType;
import org.springframework.stereotype.Component;
import org.springframework.web.filter.OncePerRequestFilter;
import javax.servlet.FilterChain;
import javax.servlet.ServletException;
import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

@Component
class AdminAuthFilter extends OncePerRequestFilter {
    static final String SESSION_ATTRIBUTE = AdminAuthFilter.class.getName() + ".session";
    private final AdminAuthService auth;
    AdminAuthFilter(AdminAuthService auth) { this.auth = auth; }

    @Override protected boolean shouldNotFilter(HttpServletRequest request) {
        return !request.getRequestURI().startsWith("/v1/admin/") || request.getRequestURI().equals("/v1/admin/auth/login") || "OPTIONS".equals(request.getMethod());
    }
    @Override protected void doFilterInternal(HttpServletRequest request, HttpServletResponse response, FilterChain chain) throws ServletException, IOException {
        try {
            request.setAttribute(SESSION_ATTRIBUTE, auth.authenticate(request.getHeader("Authorization")));
            chain.doFilter(request, response);
        } catch (AdminException exception) {
            response.setStatus("unavailable".equals(exception.code) ? 503 : 401);
            response.setCharacterEncoding(StandardCharsets.UTF_8.name()); response.setContentType(MediaType.APPLICATION_JSON_VALUE);
            response.getWriter().write("{\"error\":{\"code\":\"" + exception.code + "\",\"message\":\"管理端认证不可用或会话无效\"}}");
        }
    }
    static AdminModels.AdminSession session(HttpServletRequest request) {
        AdminModels.AdminSession session = (AdminModels.AdminSession) request.getAttribute(SESSION_ATTRIBUTE);
        if (session == null) throw AdminException.unauthorized();
        return session;
    }
}
