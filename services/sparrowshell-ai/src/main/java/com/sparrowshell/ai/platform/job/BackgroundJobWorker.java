package com.sparrowshell.ai.platform.job;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.context.annotation.Configuration;
import org.springframework.scheduling.annotation.EnableScheduling;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

@Component
@ConditionalOnProperty(prefix = "sparrowshell.platform.worker", name = "enabled", havingValue = "true")
class BackgroundJobWorker {
    private static final Logger LOG = LoggerFactory.getLogger(BackgroundJobWorker.class);
    private final BackgroundJobService service;
    private final String workerId;

    BackgroundJobWorker(BackgroundJobService service,
                        @Value("${sparrowshell.platform.worker.id:local-worker}") String workerId) {
        this.service = service;
        this.workerId = workerId;
    }

    @Scheduled(fixedDelayString = "${sparrowshell.platform.worker.poll-ms:2000}")
    void poll() {
        try {
            service.runNext(workerId);
        } catch (RuntimeException exception) {
            // A failed settlement remains visible for lease recovery; payloads are never logged.
            LOG.error("Background job worker iteration failed: {}", exception.getClass().getSimpleName());
        }
    }

    @Configuration
    @EnableScheduling
    @ConditionalOnProperty(prefix = "sparrowshell.platform.worker", name = "enabled", havingValue = "true")
    static class SchedulingConfiguration {
    }
}
