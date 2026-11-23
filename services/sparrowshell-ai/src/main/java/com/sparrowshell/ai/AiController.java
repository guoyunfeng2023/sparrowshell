package com.sparrowshell.ai;

import com.sparrowshell.ai.platform.billing.AiTaskQuotaService;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.servlet.mvc.method.annotation.SseEmitter;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.Supplier;

@RestController
@RequestMapping("/v1/ai")
public class AiController {
    public static final String ANONYMOUS_ID_HEADER = "X-SparrowShell-Anonymous-Id";
    public static final String DEVICE_TOKEN_HEADER = "X-SparrowShell-Device-Token";
    private final ProviderRouter router;
    private final AiStreamService streamService;
    private final AiUsageService usageService;
    private final AiIdentityService identityService;
    private final AiTaskQuotaService quotaService;

    @org.springframework.beans.factory.annotation.Autowired
    public AiController(ProviderRouter router, AiStreamService streamService, AiUsageService usageService,
                        AiIdentityService identityService, ObjectProvider<AiTaskQuotaService> quotaService) {
        this.router = router; this.streamService = streamService; this.usageService = usageService;
        this.identityService = identityService; this.quotaService = quotaService.getIfAvailable();
    }

    AiController(ProviderRouter router) { this(router, null, null, null, (AiTaskQuotaService) null); }
    AiController(ProviderRouter router, AiStreamService streamService) { this(router, streamService, null, null, (AiTaskQuotaService) null); }
    AiController(ProviderRouter router, AiStreamService streamService, AiUsageService usageService,
                 AiIdentityService identityService) { this(router, streamService, usageService, identityService, (AiTaskQuotaService) null); }
    AiController(ProviderRouter router, AiStreamService streamService, AiUsageService usageService,
                         AiIdentityService identityService, AiTaskQuotaService quotaService) {
        this.router = router; this.streamService = streamService; this.usageService = usageService;
        this.identityService = identityService; this.quotaService = quotaService;
    }

    @PostMapping("/proposals")
    public ProposalResponse proposals(@RequestBody ProposalRequest request,
            @RequestHeader(value = "Authorization", required = false) String authorization,
            @RequestHeader(value = ANONYMOUS_ID_HEADER, required = false) String anonymousId,
            @RequestHeader(value = DEVICE_TOKEN_HEADER, required = false) String deviceToken) {
        Supplier<ProposalResponse> provider = () -> usageService == null ? router.propose(request)
                : usageService.propose(subject(authorization, anonymousId), request, () -> router.propose(request));
        return quotaService == null ? provider.get() : quotaService.propose(anonymousId, deviceToken, request, provider);
    }

    @PostMapping(value = "/proposals/stream", produces = "text/event-stream")
    public SseEmitter proposalStream(@RequestBody ProposalRequest request,
            @RequestHeader(value = "Authorization", required = false) String authorization,
            @RequestHeader(value = ANONYMOUS_ID_HEADER, required = false) String anonymousId) {
        if (streamService == null) throw new ProviderException("provider_unavailable", "流式服务未配置");
        if (quotaService != null && quotaService.isEnabled())
            throw new ProviderException("invalid_request", "启用次数额度时请使用非流式提案接口");
        return usageService == null ? streamService.stream(request)
                : streamService.stream(subject(authorization, anonymousId), request);
    }

    @PostMapping("/analysis")
    public AnalysisResponse analysis(@RequestBody AnalysisRequest request,
            @RequestHeader(value = "Authorization", required = false) String authorization,
            @RequestHeader(value = ANONYMOUS_ID_HEADER, required = false) String anonymousId,
            @RequestHeader(value = DEVICE_TOKEN_HEADER, required = false) String deviceToken) {
        Supplier<AnalysisResponse> provider = () -> usageService == null ? router.analyze(request)
                : usageService.analyze(subject(authorization, anonymousId), request, () -> router.analyze(request));
        return quotaService == null ? provider.get() : quotaService.analyze(anonymousId, deviceToken, request, provider);
    }

    @PostMapping("/agent/turn")
    public AgentTurnResponse agentTurn(@RequestBody AgentTurnRequest request,
            @RequestHeader(value = "Authorization", required = false) String authorization,
            @RequestHeader(value = ANONYMOUS_ID_HEADER, required = false) String anonymousId,
            @RequestHeader(value = DEVICE_TOKEN_HEADER, required = false) String deviceToken) {
        Supplier<AgentTurnResponse> provider = () -> usageService == null ? router.agentTurn(request)
                : usageService.agentTurn(subject(authorization, anonymousId), request, () -> router.agentTurn(request));
        return quotaService == null ? provider.get() : quotaService.agentTurn(anonymousId, deviceToken, request, provider);
    }

    private String subject(String authorization, String anonymousId) {
        if (identityService == null) throw new ProviderException("usage_unavailable", "AI 身份服务尚未配置");
        return identityService.subject(authorization, anonymousId);
    }

    @GetMapping("/health")
    public Map<String, Object> health() {
        Map<String, Object> response = new LinkedHashMap<String, Object>();
        response.put("status", "ok"); response.put("providerConfigured", router.isConfigured());
        response.put("providers", router.configuration()); return response;
    }

    @ExceptionHandler(ProviderException.class)
    ResponseEntity<Map<String, Object>> providerError(ProviderException exception) {
        HttpStatus status;
        switch (exception.code()) {
            case "invalid_request":
            case "invalid_identity": status = HttpStatus.BAD_REQUEST; break;
            case "unauthorized": status = HttpStatus.UNAUTHORIZED; break;
            case "duplicate_request":
            case "duplicate_task": status = HttpStatus.CONFLICT; break;
            case "quota_exceeded": status = HttpStatus.PAYMENT_REQUIRED; break;
            case "rate_limit": status = HttpStatus.TOO_MANY_REQUESTS; break;
            case "timeout": status = HttpStatus.GATEWAY_TIMEOUT; break;
            case "invalid_provider_response": status = HttpStatus.BAD_GATEWAY; break;
            case "interactive_command": status = HttpStatus.UNPROCESSABLE_ENTITY; break;
            default: status = HttpStatus.SERVICE_UNAVAILABLE;
        }
        return ResponseEntity.status(status).body(error(exception.code(), exception.getMessage()));
    }

    @ExceptionHandler(Exception.class)
    ResponseEntity<Map<String, Object>> invalidRequest(Exception exception) {
        return ResponseEntity.badRequest().body(error("invalid_request", "请求格式无效"));
    }

    private Map<String, Object> error(String code, String message) {
        Map<String, Object> body = new LinkedHashMap<String, Object>(); body.put("code", code);
        body.put("message", message); body.put("requestId", null); return body;
    }
}
