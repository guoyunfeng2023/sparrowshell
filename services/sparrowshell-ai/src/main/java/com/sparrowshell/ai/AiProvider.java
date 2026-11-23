package com.sparrowshell.ai;

/** Server-side model adapter. It never receives SSH authority or execution targets. */
interface AiProvider {
    ProposalResponse propose(ProposalRequest request);
    AnalysisResponse analyze(AnalysisRequest request);
    boolean isConfigured();
    String providerName();
    String proposalRouteId();
    String analysisRouteId();
}
