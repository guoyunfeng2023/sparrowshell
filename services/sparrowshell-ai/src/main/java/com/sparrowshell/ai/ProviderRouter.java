package com.sparrowshell.ai;

import org.springframework.stereotype.Component;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

/** Selects one pinned route. Provider failure never causes an in-request fallback. */
@Component
class ProviderRouter {
    private final QwenProvider qwen;
    private final Map<String, AiProvider> proposalRoutes;
    private final Map<String, AiProvider> analysisRoutes;

    ProviderRouter(DeepSeekProvider deepSeek, QwenProvider qwen) {
        this.qwen = qwen;
        Map<String, AiProvider> proposals = new LinkedHashMap<String, AiProvider>();
        Map<String, AiProvider> analyses = new LinkedHashMap<String, AiProvider>();
        for (AiProvider provider : new AiProvider[]{deepSeek, qwen}) {
            proposals.put(provider.proposalRouteId(), provider);
            analyses.put(provider.analysisRouteId(), provider);
        }
        this.proposalRoutes = Collections.unmodifiableMap(proposals);
        this.analysisRoutes = Collections.unmodifiableMap(analyses);
    }

    ProposalResponse propose(ProposalRequest request) {
        String routeId = route(request == null ? null : request.routeId, qwen.proposalRouteId());
        AiProvider provider = proposalRoutes.get(routeId);
        if (provider == null) throw new ProviderException("route_unavailable", "请求的模型路由不可用");
        return provider.propose(request);
    }

    AnalysisResponse analyze(AnalysisRequest request) {
        String routeId = route(request == null ? null : request.routeId, qwen.analysisRouteId());
        AiProvider provider = analysisRoutes.get(routeId);
        if (provider == null) throw new ProviderException("route_unavailable", "请求的模型路由不可用");
        return provider.analyze(request);
    }

    AgentTurnResponse agentTurn(AgentTurnRequest request) {
        String routeId = route(request == null ? null : request.routeId, QwenProvider.AGENT_ROUTE);
        if (!QwenProvider.AGENT_ROUTE.equals(routeId))
            throw new ProviderException("route_unavailable", "动态 Agent 只支持固定通义千问路由");
        return qwen.agentTurn(request);
    }

    boolean isConfigured() { return qwen.isConfigured(); }

    Map<String, Boolean> configuration() {
        Map<String, Boolean> result = new LinkedHashMap<String, Boolean>();
        for (AiProvider provider : proposalRoutes.values()) result.put(provider.providerName(), provider.isConfigured());
        return result;
    }

    private String route(String requested, String defaultRoute) {
        return requested == null || requested.trim().isEmpty() ? defaultRoute : requested.trim();
    }
}
