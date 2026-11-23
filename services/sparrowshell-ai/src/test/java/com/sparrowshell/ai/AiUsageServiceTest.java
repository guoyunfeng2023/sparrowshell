package com.sparrowshell.ai;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import java.math.BigDecimal;
import java.time.Instant;
import java.util.*;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Supplier;
import static org.junit.jupiter.api.Assertions.*;

class AiUsageServiceTest {
    private final FakeRepository repository = new FakeRepository();
    private final AiUsageService service = new AiUsageService(repository, new ObjectMapper(), true, true,
            "local-test-subject", new BigDecimal("2.00"), new BigDecimal("0.05"),
            new BigDecimal("1.00"), new BigDecimal("2.00"), "test-prices-v1");

    @Test void duplicateClientRequestCannotReserveOrCallProviderTwice() {
        AtomicInteger calls = new AtomicInteger();
        ProposalRequest request = proposal("5c4e6dca-e41b-4a08-a9af-c256cb67c676");
        service.propose(request, () -> response(calls, usage(100, 50)));
        ProviderException duplicate = assertThrows(ProviderException.class,
                () -> service.propose(request, () -> response(calls, usage(100, 50))));
        assertEquals("duplicate_request", duplicate.code());
        assertEquals(1, calls.get());
        assertEquals(1, repository.reserveCalls.get());
        assertEquals(new BigDecimal("0.00020000"), repository.only().actualCost);
        assertEquals("test-prices-v1", repository.only().priceVersion);
    }

    @Test void insufficientQuotaFailsBeforeProviderCall() {
        AiUsageService limited = new AiUsageService(repository, new ObjectMapper(), true, true,
                "local-test-subject", new BigDecimal("0.01"), new BigDecimal("0.05"),
                new BigDecimal("1.00"), new BigDecimal("2.00"), "test-prices-v1");
        AtomicInteger calls = new AtomicInteger();
        ProviderException error = assertThrows(ProviderException.class, () -> limited.propose(
                proposal("5c4e6dca-e41b-4a08-a9af-c256cb67c676"), () -> response(calls, usage(1, 1))));
        assertEquals("quota_exceeded", error.code());
        assertEquals(0, calls.get());
        assertNull(repository.only);
    }

    @Test void missingUsageIsUnknownAndNeverZeroCost() {
        service.propose(proposal("5c4e6dca-e41b-4a08-a9af-c256cb67c676"),
                () -> response(new AtomicInteger(), Collections.emptyMap()));
        AiUsage saved = repository.only();
        assertEquals("unknown", saved.usageStatus);
        assertNull(saved.actualCost);
        assertNull(saved.inputTokens);
        assertNull(saved.outputTokens);
        assertEquals(new BigDecimal("0.00000000"), repository.reservedCost);
    }

    @Test void providerFailureReleasesReservationAndPreservesOriginalError() {
        ProviderException error = assertThrows(ProviderException.class, () -> service.propose(
                proposal("5c4e6dca-e41b-4a08-a9af-c256cb67c676"), () -> {
                    throw new ProviderException("timeout", "provider timeout");
                }));
        assertEquals("timeout", error.code());
        assertEquals("failed", repository.only().status);
        assertEquals("unknown", repository.only().usageStatus);
        assertNull(repository.only().actualCost);
        assertEquals(new BigDecimal("0.00000000"), repository.reservedCost);
    }

    @Test void providerCallIsOutsideRepositoryTransactions() {
        service.propose(proposal("5c4e6dca-e41b-4a08-a9af-c256cb67c676"), () -> {
            assertFalse(repository.transactionOpen);
            return response(new AtomicInteger(), usage(10, 5));
        });
        assertFalse(repository.transactionOpen);
    }

    @Test void agentTurnUsesIndependentOperationAndSettlesActualUsage() {
        AgentTurnRequest request = new AgentTurnRequest();
        request.clientRequestId = "5c4e6dca-e41b-4a08-a9af-c256cb67c676";
        request.routeId = "deepseek.chat.agent";
        request.intent = "检查 Docker";
        request.context = Collections.emptyMap();
        AgentTurnResponse response = service.agentTurn(request, () -> {
            AgentTurnResponse value = new AgentTurnResponse();
            value.requestId = UUID.randomUUID().toString();
            value.usage = usage(20, 10);
            return value;
        });
        assertNotNull(response);
        assertEquals("agent_turn", repository.only().operation);
        assertEquals("succeeded", repository.only().status);
        assertEquals(new BigDecimal("0.00004000"), repository.only().actualCost);
    }

    @Test void explicitAuthenticatedOrAnonymousSubjectOwnsTheReservation() {
        String subject = "anonymous:d2d5bafa-76ce-4a72-9db7-a82f527dd17b";
        service.propose(subject, proposal("5c4e6dca-e41b-4a08-a9af-c256cb67c676"),
                () -> response(new AtomicInteger(), usage(10, 5)));
        assertEquals(subject, repository.only().accountId);
        assertNotEquals("local-test-subject", repository.only().accountId);
    }

    @Test void requiredIdentityAndRequestIdCannotBeBypassed() {
        ProposalRequest request = proposal(null);
        ProviderException error = assertThrows(ProviderException.class,
                () -> service.propose(request, () -> response(new AtomicInteger(), usage(1, 1))));
        assertEquals("invalid_request", error.code());
        assertEquals(0, repository.reserveCalls.get());
    }

    private ProposalRequest proposal(String id) {
        ProposalRequest request = new ProposalRequest();
        request.clientRequestId = id;
        request.routeId = "deepseek.chat.completions";
        request.intent = "检查磁盘";
        request.context = Collections.emptyMap();
        return request;
    }

    private ProposalResponse response(AtomicInteger calls, Map<String, Object> usage) {
        calls.incrementAndGet();
        ProposalResponse response = new ProposalResponse();
        response.requestId = UUID.randomUUID().toString();
        response.routeId = "deepseek.chat.completions";
        response.provider = "deepseek";
        response.model = "deepseek-chat";
        response.usage = usage;
        return response;
    }

    private Map<String, Object> usage(int input, int output) {
        Map<String, Object> usage = new HashMap<String, Object>();
        usage.put("prompt_tokens", input);
        usage.put("completion_tokens", output);
        return usage;
    }

    static class FakeRepository implements AiUsageLedgerRepository {
        final AtomicInteger reserveCalls = new AtomicInteger();
        boolean transactionOpen;
        BigDecimal reservedCost = new BigDecimal("0.00000000");
        AiUsage only;

        AiUsage only() { return only; }

        @Override public synchronized AiUsage reserve(AiUsage requested, BigDecimal quota) {
            reserveCalls.incrementAndGet();
            transactionOpen = true;
            if (only != null) {
                transactionOpen = false;
                return only.copy(false);
            }
            if (reservedCost.add(requested.estimatedCost).compareTo(quota) > 0)
                throw new ProviderException("quota_exceeded", "AI 配额不足");
            only = requested.copy(false);
            reservedCost = reservedCost.add(requested.estimatedCost);
            transactionOpen = false;
            return only.copy(true);
        }

        @Override public synchronized void settle(AiUsage settlement) {
            transactionOpen = true;
            AiUsage current = only;
            if (!"reserved".equals(current.status)) { transactionOpen = false; return; }
            reservedCost = reservedCost.subtract(current.estimatedCost);
            if ("succeeded".equals(settlement.status) && settlement.actualCost != null)
                reservedCost = reservedCost.add(settlement.actualCost);
            settlement.priceVersion = current.priceVersion;
            only = settlement.copy(false);
            transactionOpen = false;
        }

        @Override public synchronized Optional<AiUsage> find(String accountId, String clientRequestId) {
            return Optional.ofNullable(only == null ? null : only.copy(false));
        }

        @Override public synchronized List<AiUsage> between(Instant from, Instant to) {
            return only == null ? Collections.<AiUsage>emptyList() : Collections.singletonList(only.copy(false));
        }
    }
}
