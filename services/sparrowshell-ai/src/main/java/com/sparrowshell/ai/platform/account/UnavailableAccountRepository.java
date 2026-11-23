package com.sparrowshell.ai.platform.account;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Repository;

@Repository
@ConditionalOnProperty(prefix = "sparrowshell.platform", name = "persistence-enabled", havingValue = "false", matchIfMissing = true)
class UnavailableAccountRepository implements AccountRepository {
}
