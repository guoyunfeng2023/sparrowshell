package com.sparrowshell.ai.platform.billing;

import com.sparrowshell.ai.*;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import java.time.Instant;
import java.time.YearMonth;
import java.time.ZoneId;
import java.util.UUID;
import static org.junit.jupiter.api.Assertions.*;

class AiTaskQuotaServiceTest {
    private static final String DEVICE = "0be0dd37-9379-4663-a94f-799f16ab7678";
    private InMemoryBillingRepository repository;
    private AiTaskQuotaService service;

    @BeforeEach void setUp() { repository = new InMemoryBillingRepository(); service = new AiTaskQuotaService(repository, true, 15); }

    @Test void dynamicAgentConsumesExactlyOnceAcrossToolTurnsEvenWhenFull() {
        String month = YearMonth.now(ZoneId.of("Asia/Shanghai")).toString(); repository.freeUsed.put(DEVICE + month, 49);
        String task = UUID.randomUUID().toString(); AgentTurnRequest request = request(task);
        AgentTurnResponse tool = response("tool_call"); AgentTurnResponse answer = response("final_answer");
        assertSame(tool, service.agentTurn(DEVICE, null, request, () -> tool));
        assertEquals(50, repository.entitlement(DEVICE, null, Instant.now()).used);
        assertSame(answer, service.agentTurn(DEVICE, null, request, () -> answer));
        assertEquals(50, repository.entitlement(DEVICE, null, Instant.now()).used);
        assertEquals(1, repository.tasks.size());
    }

    @Test void failuresTimeoutsAndInvalidDecisionsReleaseTheReservation() {
        for (int index = 0; index < 3; index++) {
            String task = UUID.randomUUID().toString(); AgentTurnRequest request = request(task);
            if (index == 0) assertThrows(ProviderException.class, () -> service.agentTurn(DEVICE, null, request, () -> { throw new ProviderException("timeout", "timeout"); }));
            else if (index == 1) assertThrows(ProviderException.class, () -> service.agentTurn(DEVICE, null, request, () -> { throw new ProviderException("invalid_provider_response", "invalid"); }));
            else assertThrows(ProviderException.class, () -> service.agentTurn(DEVICE, null, request, () -> response("other")));
            assertEquals(0, repository.entitlement(DEVICE, null, Instant.now()).used);
        }
    }

    @Test void proposalAndAnalysisEachConsumeOneAndExhaustionUsesDedicatedError() {
        ProposalRequest proposal = new ProposalRequest(); proposal.clientRequestId = UUID.randomUUID().toString();
        ProposalResponse proposalResponse = new ProposalResponse();
        assertSame(proposalResponse, service.propose(DEVICE, null, proposal, () -> proposalResponse));
        AnalysisRequest analysis = new AnalysisRequest(); analysis.clientRequestId = UUID.randomUUID().toString(); AnalysisResponse analysisResponse = new AnalysisResponse();
        assertSame(analysisResponse, service.analyze(DEVICE, null, analysis, () -> analysisResponse));
        assertEquals(2, repository.entitlement(DEVICE, null, Instant.now()).used);
        repository.freeUsed.put(DEVICE + YearMonth.now(ZoneId.of("Asia/Shanghai")), 50);
        ProviderException error = assertThrows(ProviderException.class, () -> service.agentTurn(DEVICE, null, request(UUID.randomUUID().toString()), () -> response("final_answer")));
        assertEquals("quota_exceeded", error.code());
    }

    @Test void manualStopReleasesAndExpiredReservationDoesNotLeakQuota() {
        String task = UUID.randomUUID().toString(); service.agentTurn(DEVICE, null, request(task), () -> response("tool_call"));
        assertEquals(1, repository.entitlement(DEVICE, null, Instant.now()).used);
        service.release(DEVICE, task); assertEquals(0, repository.entitlement(DEVICE, null, Instant.now()).used);
        String expired = UUID.randomUUID().toString(); service.agentTurn(DEVICE, null, request(expired), () -> response("tool_call"));
        repository.tasks.get(DEVICE + expired).expiresAt = Instant.now().minusSeconds(1);
        assertEquals(0, repository.entitlement(DEVICE, null, Instant.now()).used);
    }

    @Test void disabledBillingKeepsLegacyBehavior() {
        AiTaskQuotaService disabled = new AiTaskQuotaService(repository, false, 15); AgentTurnResponse answer = response("final_answer");
        assertSame(answer, disabled.agentTurn(null, null, new AgentTurnRequest(), () -> answer)); assertTrue(repository.tasks.isEmpty());
    }

    private static AgentTurnRequest request(String task) { AgentTurnRequest request = new AgentTurnRequest(); request.taskId = task; return request; }
    private static AgentTurnResponse response(String type) { AgentTurnResponse response = new AgentTurnResponse(); response.type = type; return response; }
}
