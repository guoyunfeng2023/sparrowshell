package com.sparrowshell.ai.platform.admin;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.ApplicationArguments;
import org.springframework.boot.ApplicationRunner;
import org.springframework.boot.autoconfigure.condition.ConditionalOnExpression;
import org.springframework.stereotype.Component;

@Component
@ConditionalOnExpression("${sparrowshell.platform.persistence-enabled:false} and ${sparrowshell.admin.bootstrap.enabled:false}")
class AdminBootstrap implements ApplicationRunner {
    private static final Logger LOG = LoggerFactory.getLogger(AdminBootstrap.class);
    private final AdminAuthService auth;
    private final String email;
    private final String password;
    private final String mfaSecret;
    AdminBootstrap(AdminAuthService auth, @Value("${sparrowshell.admin.bootstrap.email:}") String email,
                   @Value("${sparrowshell.admin.bootstrap.password:}") String password,
                   @Value("${sparrowshell.admin.bootstrap.mfa-secret:}") String mfaSecret) {
        this.auth = auth; this.email = email; this.password = password; this.mfaSecret = mfaSecret;
    }
    @Override public void run(ApplicationArguments args) {
        if (email.isEmpty() || password.isEmpty() || mfaSecret.isEmpty()) throw new IllegalStateException("显式管理员引导需要完整的邮箱、密码和 MFA 密钥配置");
        AdminModels.AdminUser admin = auth.bootstrap(email, password, mfaSecret);
        LOG.info("Initial administrator created for {}", admin.email);
    }
}
