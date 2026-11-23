package com.sparrowshell.ai;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.boot.autoconfigure.jdbc.DataSourceAutoConfiguration;
import org.springframework.boot.autoconfigure.jdbc.DataSourceTransactionManagerAutoConfiguration;
import org.springframework.boot.autoconfigure.jdbc.JdbcTemplateAutoConfiguration;
import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Import;

@Configuration
@ConditionalOnProperty(prefix = "sparrowshell.usage", name = "enabled", havingValue = "true")
@Import({DataSourceAutoConfiguration.class, JdbcTemplateAutoConfiguration.class,
        DataSourceTransactionManagerAutoConfiguration.class})
class AiUsagePersistenceConfiguration {
}
