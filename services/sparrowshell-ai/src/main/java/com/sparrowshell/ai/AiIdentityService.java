package com.sparrowshell.ai;

import com.sparrowshell.ai.platform.account.AccountException;
import com.sparrowshell.ai.platform.account.AccountService;
import org.springframework.stereotype.Component;
import java.util.UUID;

/** Resolves an AI quota identity without granting access to account cloud sync. */
@Component
class AiIdentityService {
    private final AccountService accountService;

    AiIdentityService(AccountService accountService) {
        this.accountService = accountService;
    }

    String subject(String authorization, String anonymousId) {
        if (authorization != null) {
            try {
                return accountService.authenticateAccountId(authorization);
            } catch (AccountException exception) {
                throw new ProviderException("unauthorized", "登录状态已失效，请重新登录；也可退出登录后匿名使用 AI");
            }
        }
        try {
            UUID uuid = UUID.fromString(anonymousId == null ? "" : anonymousId.trim());
            return "anonymous:" + uuid.toString();
        } catch (IllegalArgumentException exception) {
            throw new ProviderException("invalid_identity", "匿名用户标识无效，请重新启动客户端后重试");
        }
    }
}
