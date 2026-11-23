package com.sparrowshell.ai;

import org.springframework.stereotype.Component;
import org.springframework.web.servlet.mvc.method.annotation.SseEmitter;
import javax.annotation.PreDestroy;
import java.io.IOException;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.FutureTask;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/** Bounded SSE worker. Only a fully validated ProposalResponse can reach final. */
@Component
class AiStreamService {
    private final ProviderRouter router;
    private final AiUsageService usageService;
    private final ThreadPoolExecutor executor;

    AiStreamService(ProviderRouter router) { this(router, null); }

    @org.springframework.beans.factory.annotation.Autowired
    AiStreamService(ProviderRouter router, AiUsageService usageService) {
        this.router = router;
        this.usageService = usageService;
        this.executor = new ThreadPoolExecutor(2, 4, 30, TimeUnit.SECONDS,
                new ArrayBlockingQueue<Runnable>(32), runnable -> {
            Thread thread = new Thread(runnable, "sparrowshell-ai-sse");
            thread.setDaemon(true);
            return thread;
        }, new ThreadPoolExecutor.AbortPolicy());
    }

    SseEmitter stream(ProposalRequest request) {
        return stream(null, request);
    }

    SseEmitter stream(String subject, ProposalRequest request) {
        SseEmitter emitter = new SseEmitter(125000L);
        FutureTask<Void> task = new FutureTask<>(() -> {
            try {
                ProposalResponse response = usageService == null ? router.propose(request)
                        : usageService.propose(subject, request, () -> router.propose(request));
                Map<String, Object> meta = new LinkedHashMap<String, Object>();
                meta.put("requestId", response.requestId);
                meta.put("routeId", response.routeId);
                emitter.send(SseEmitter.event().name("meta").data(meta));
                emitter.send(SseEmitter.event().name("final").data(response));
                emitter.complete();
            } catch (ProviderException exception) {
                sendError(emitter, exception.code(), Arrays.asList("timeout", "rate_limit", "provider_unavailable").contains(exception.code()));
            } catch (Exception exception) {
                sendError(emitter, "provider_unavailable", true);
            }
            return null;
        });
        emitter.onCompletion(() -> task.cancel(true));
        emitter.onTimeout(() -> task.cancel(true));
        emitter.onError(ignored -> task.cancel(true));
        try {
            executor.execute(task);
        } catch (RejectedExecutionException exception) {
            sendError(emitter, "server_rate_limit", true);
        }
        return emitter;
    }

    private void sendError(SseEmitter emitter, String code, boolean retryable) {
        try {
            Map<String, Object> error = new LinkedHashMap<String, Object>();
            error.put("code", code);
            error.put("retryable", retryable);
            emitter.send(SseEmitter.event().name("error").data(error));
            emitter.complete();
        } catch (IOException | IllegalStateException exception) {
            emitter.completeWithError(exception);
        }
    }

    @PreDestroy void close() { executor.shutdownNow(); }
}
